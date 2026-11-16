use super::*;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_RANGE, ETAG, IF_RANGE, RANGE},
};
use std::{
    io::{Seek, SeekFrom},
    time::{Duration, Instant},
};
const CHUNK: u64 = 1024 * 1024;
const MAX_FILE: u64 = 16 * 1024 * 1024 * 1024 * 1024;
struct Rate {
    epoch: Instant,
    bytes: u64,
    limit: u64,
}
fn status(control: &Control, text: &str) {
    *control.status.lock().unwrap() = text.into();
}
async fn stopped(control: &Control) {
    while !control.stop.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn send(
    request: reqwest::RequestBuilder,
    control: &Control,
) -> Result<reqwest::Response, String> {
    tokio::select! {
        _=stopped(control)=>Err("Paused".into()),
        response=tokio::time::timeout(Duration::from_secs(25),request.send())=>response.map_err(|_|"Server response timed out".to_string())?.map_err(|error|error.without_url().to_string()),
    }
}
async fn bytes(
    response: &mut reqwest::Response,
    control: &Control,
) -> Result<Option<::bytes::Bytes>, String> {
    tokio::select! {
        _=stopped(control)=>Err("Paused".into()),
        chunk=tokio::time::timeout(Duration::from_secs(20),response.chunk())=>chunk.map_err(|_|"Transfer stalled".to_string())?.map_err(|error|error.without_url().to_string()),
    }
}
fn identity(response: &reqwest::Response) -> Result<(), String> {
    if response
        .headers()
        .get(CONTENT_ENCODING)
        .is_some_and(|value| value.as_bytes() != b"identity")
    {
        return Err(
            "Server returned encoded bytes despite requesting identity; transfer stopped".into(),
        );
    }
    Ok(())
}
fn etag(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(ETAG)
        .and_then(|v| v.to_str().ok())
        .filter(|s| {
            s.starts_with('"') && s.ends_with('"') && s.len() <= 1024 && !s.starts_with("W/")
        })
        .map(str::to_owned)
}
fn content_range(response: &reqwest::Response) -> Result<(u64, u64, u64), String> {
    let value = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .ok_or("Missing Content-Range")?;
    let (range, total) = value
        .strip_prefix("bytes ")
        .ok_or("Invalid range unit")?
        .split_once('/')
        .ok_or("Invalid Content-Range")?;
    let (start, end) = range
        .split_once('-')
        .ok_or("Invalid Content-Range bounds")?;
    let parse = |s: &str| {
        s.parse::<u64>()
            .map_err(|_| "Invalid Content-Range number".to_string())
    };
    let (start, end, total) = (parse(start)?, parse(end)?, parse(total)?);
    if start > end || end >= total || total > MAX_FILE {
        return Err("Invalid Content-Range size".into());
    }
    Ok((start, end, total))
}
async fn throttle(control: &Control, rate: &Mutex<Rate>, count: u64) -> Result<(), String> {
    let delay = {
        let mut rate = rate.lock().unwrap();
        let limit = control.limit.load(Ordering::Relaxed);
        if limit == 0 {
            rate.epoch = Instant::now();
            rate.bytes = 0;
            rate.limit = 0;
            Duration::ZERO
        } else {
            if rate.limit != limit {
                rate.epoch = Instant::now();
                rate.bytes = 0;
                rate.limit = limit;
            }
            rate.bytes = rate.bytes.saturating_add(count);
            Duration::from_secs_f64(rate.bytes as f64 / limit as f64)
                .saturating_sub(rate.epoch.elapsed())
        }
    };
    tokio::select! { _=stopped(control)=>Err("Paused".into()), _=tokio::time::sleep(delay)=>Ok(()) }
}
fn checkpoint(stage: &Path, manifest: &Mutex<Manifest>) -> Result<(), String> {
    let manifest = manifest.lock().unwrap();
    crate::profile::write_record(&stage.join("manifest.json"), &*manifest)
}

pub(super) async fn run(job: Job, control: Arc<Control>) -> Result<String, String> {
    job.validate()?;
    let parent = job
        .destination
        .parent()
        .ok_or("Missing destination parent")?;
    if fs::canonicalize(parent).map_err(|e| e.to_string())? != parent {
        return Err(
            "Destination parent changed or contains symlinks; choose its canonical path".into(),
        );
    }
    let stage = job.stage();
    if job.destination.symlink_metadata().is_ok() && !stage.exists() {
        return Err("Destination already exists; it will not be overwritten".into());
    }
    crate::profile::ensure_dir(&stage)?;
    let lock_path = stage.join("lock");
    let lock = if lock_path.exists() {
        open(&lock_path, false)?
    } else {
        open(&lock_path, true)?
    };
    lock.try_lock()
        .map_err(|_| "This transfer is already running in another process")?;
    if job.destination.symlink_metadata().is_ok() {
        status(&control, "Recovering completed file");
        let receipt: Receipt = crate::profile::read_record(&stage.join("receipt.json"))?
            .ok_or("Destination exists without a completion receipt; it was not overwritten")?;
        if receipt.destination != job.destination
            || receipt.size > MAX_FILE
            || receipt.digest.len() != 64
            || job
                .sha256
                .as_ref()
                .is_some_and(|expected| expected != &receipt.digest)
        {
            return Err("Completion receipt does not match this job".into());
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&job.destination)
            .map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file()
            || metadata.len() != receipt.size
            || hex(&hash(&mut file, receipt.size, Some(&control))?) != receipt.digest
        {
            return Err("Completed file changed or was corrupted; recovery refused".into());
        }
        control.received.store(receipt.size, Ordering::Relaxed);
        control.total.store(receipt.size, Ordering::Relaxed);
        return Ok(receipt.digest);
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 {
                return attempt.error("Too many redirects");
            }
            if validate_url(attempt.url().as_str()).is_err()
                || (attempt
                    .previous()
                    .first()
                    .is_some_and(|url| url.scheme() == "https")
                    && attempt.url().scheme() != "https")
            {
                return attempt.error("Unsafe download redirect");
            }
            attempt.follow()
        }))
        .user_agent("Paisley/0.1 managed-download")
        .build()
        .map_err(|_| "Could not initialize TLS/HTTP client")?;
    let mut probe = send(
        client
            .get(&job.url)
            .header(ACCEPT_ENCODING, "identity")
            .header(RANGE, "bytes=0-0"),
        &control,
    )
    .await?;
    identity(&probe)?;
    let strong = etag(&probe);
    let total = if probe.status() == StatusCode::PARTIAL_CONTENT {
        let (start, end, total) = content_range(&probe)?;
        if start != 0 || end != 0 {
            return Err("Server returned an unexpected probe range".into());
        }
        Some(total)
    } else if probe.status() == StatusCode::RANGE_NOT_SATISFIABLE
        && probe
            .headers()
            .get(CONTENT_RANGE)
            .is_some_and(|value| value.as_bytes() == b"bytes */0")
    {
        Some(0)
    } else if probe.status().is_success() {
        probe.content_length()
    } else {
        return Err(format!("Server returned HTTP {}", probe.status().as_u16()));
    };
    if total.is_some_and(|n| n > MAX_FILE) {
        return Err("File exceeds the supported 16 TiB size".into());
    }
    let final_url = probe.url().to_string();
    let ranged = probe.status() == StatusCode::PARTIAL_CONTENT && strong.is_some();
    // Do not retain the probe response body or buffer a server ignoring Range.
    let _ = &mut probe;
    drop(probe);
    let saved: Option<Manifest> = crate::profile::read_record(&stage.join("manifest.json"))?;
    let manifest = if let Some(saved) = saved {
        if saved.url != job.url
            || saved.final_url != final_url
            || saved.total != total
            || saved.etag != strong
            || saved.ranged != ranged
        {
            return Err(
                "Server file or validator changed. Resume refused; start a new download".into(),
            );
        }
        validate_manifest(&saved)?;
        saved
    } else {
        let count = if ranged && total.unwrap_or(0) >= 4 * 1024 * 1024 {
            job.connections
        } else {
            1
        };
        let mut parts = Vec::new();
        for index in 0..count {
            let start = total.unwrap_or(0) * index as u64 / count as u64;
            let end = total.map(|n| n * (index + 1) as u64 / count as u64);
            parts.push(Part {
                start,
                end,
                committed: 0,
                digest: hex(&Sha256::new()),
            });
        }
        Manifest {
            url: job.url.clone(),
            final_url,
            total,
            etag: strong,
            ranged,
            parts,
        }
    };
    control.total.store(total.unwrap_or(0), Ordering::Relaxed);
    let manifest = Arc::new(Mutex::new(manifest));
    checkpoint(&stage, &manifest)?;
    status(&control, "Checking partial files");
    let parts = manifest.lock().unwrap().parts.clone();
    let mut files = Vec::new();
    let mut received = 0;
    for (index, part) in parts.iter().enumerate() {
        let path = stage.join(format!("segment-{index}.part"));
        let mut file = if path.exists() {
            open(&path, false)?
        } else if part.committed == 0 {
            open(&path, true)?
        } else {
            return Err("A checkpointed segment is missing".into());
        };
        let digest = hash(&mut file, part.committed, Some(&control))?;
        if hex(&digest) != part.digest {
            return Err("A partial segment was modified or corrupted. Resume refused".into());
        }
        file.set_len(part.committed).map_err(|e| e.to_string())?;
        received += part.committed;
        files.push((file, digest));
    }
    control.received.store(received, Ordering::Relaxed);
    let rate = Arc::new(Mutex::new(Rate {
        epoch: Instant::now(),
        bytes: 0,
        limit: job.limit,
    }));
    status(
        &control,
        if ranged {
            "Downloading · verified ranges"
        } else {
            "Downloading · single connection; resume requires restart"
        },
    );
    if ranged {
        let mut tasks = tokio::task::JoinSet::new();
        for (index, (file, digest)) in files.into_iter().enumerate() {
            tasks.spawn(segment(
                index,
                file,
                digest,
                client.clone(),
                stage.clone(),
                manifest.clone(),
                control.clone(),
                rate.clone(),
            ));
        }
        let mut failure = None;
        while let Some(result) = tasks.join_next().await {
            let result = result
                .map_err(|_| "Transfer worker stopped unexpectedly".to_string())
                .and_then(|result| result);
            if let Err(error) = result {
                if failure.is_none() {
                    failure = Some(error);
                }
                control.stop.store(true, Ordering::Relaxed);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
    } else {
        let (file, _) = files.pop().unwrap();
        single(file, client, &stage, &manifest, &control, &rate).await?;
    }
    if control.stop.load(Ordering::Relaxed) {
        return Err("Paused".into());
    }
    status(&control, "Verifying and assembling");
    publish(&job, &stage, &manifest, &control)
}
fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.parts.is_empty() || manifest.parts.len() > 4 {
        return Err("Invalid segment count".into());
    }
    let mut next = 0;
    for part in &manifest.parts {
        if part.start != next
            || part
                .end
                .is_some_and(|end| end < part.start || part.committed > end - part.start)
            || part.digest.len() != 64
        {
            return Err("Invalid segment checkpoint".into());
        }
        next = part.end.unwrap_or(0);
    }
    if Some(next) != manifest.total && manifest.total.is_some() {
        return Err("Incomplete segment coverage".into());
    }
    if !manifest.ranged && (manifest.parts.len() != 1 || manifest.parts[0].committed != 0) {
        return Err("Non-resumable checkpoint has saved bytes".into());
    }
    Ok(())
}
async fn segment(
    index: usize,
    mut file: File,
    mut digest: Sha256,
    client: Client,
    stage: PathBuf,
    manifest: Arc<Mutex<Manifest>>,
    control: Arc<Control>,
    rate: Arc<Mutex<Rate>>,
) -> Result<(), String> {
    let data = manifest.lock().unwrap().clone();
    let part = &data.parts[index];
    let mut offset = part.start + part.committed;
    let end = part.end.ok_or("Missing segment end")?;
    let mut failures = 0;
    while offset < end {
        if control.stop.load(Ordering::Relaxed) {
            return Err("Paused".into());
        }
        let last = (offset + CHUNK).min(end) - 1;
        let original = digest.clone();
        file.set_len(offset - part.start)
            .map_err(|e| e.to_string())?;
        file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        let mut attempt_received = 0u64;
        let result = async {
            let mut response = send(
                client
                    .get(&data.final_url)
                    .header(ACCEPT_ENCODING, "identity")
                    .header(RANGE, format!("bytes={offset}-{last}"))
                    .header(IF_RANGE, data.etag.as_deref().unwrap()),
                &control,
            )
            .await?;
            identity(&response)?;
            if response.status() != StatusCode::PARTIAL_CONTENT
                || response.url().as_str() != data.final_url
                || etag(&response) != data.etag
                || content_range(&response)? != (offset, last, data.total.unwrap())
            {
                return Err("Server range/validator changed; no bytes were combined".to_string());
            }
            let expected = last - offset + 1;
            let mut count = 0;
            while let Some(chunk) = bytes(&mut response, &control).await? {
                if count + chunk.len() as u64 > expected {
                    return Err("Range response exceeded its declared bounds".into());
                }
                for slice in chunk.chunks(32768) {
                    throttle(&control, &rate, slice.len() as u64).await?;
                    file.write_all(slice).map_err(|e| e.to_string())?;
                    digest.update(slice);
                    count += slice.len() as u64;
                    attempt_received += slice.len() as u64;
                    control
                        .received
                        .fetch_add(slice.len() as u64, Ordering::Relaxed);
                }
            }
            if count != expected {
                return Err("Truncated range response".into());
            }
            Ok::<u64, String>(count)
        }
        .await;
        match result {
            Ok(count) => {
                file.sync_all().map_err(|e| e.to_string())?;
                {
                    let mut saved = manifest.lock().unwrap();
                    saved.parts[index].committed += count;
                    saved.parts[index].digest = hex(&digest);
                }
                checkpoint(&stage, &manifest)?;
                offset += count;
                failures = 0;
            }
            Err(error) => {
                if control.stop.load(Ordering::Relaxed) && attempt_received > 0 {
                    // Keep bytes already written when pausing, even before a full range ends.
                    file.sync_all().map_err(|e| e.to_string())?;
                    {
                        let mut saved = manifest.lock().unwrap();
                        saved.parts[index].committed += attempt_received;
                        saved.parts[index].digest = hex(&digest);
                    }
                    checkpoint(&stage, &manifest)?;
                }
                if control.stop.load(Ordering::Relaxed)
                    || failures >= 3
                    || error.contains("validator")
                    || error.contains("bounds")
                {
                    return Err(error);
                }
                digest = original;
                control
                    .received
                    .fetch_sub(attempt_received, Ordering::Relaxed);
                failures += 1;
                status(&control, &format!("Retrying connection ({failures}/3)"));
                tokio::select! { _=stopped(&control)=>return Err("Paused".into()), _=tokio::time::sleep(Duration::from_secs(1<<failures))=>{} }
            }
        }
    }
    Ok(())
}
async fn single(
    mut file: File,
    client: Client,
    stage: &Path,
    manifest: &Mutex<Manifest>,
    control: &Control,
    rate: &Mutex<Rate>,
) -> Result<(), String> {
    let url = manifest.lock().unwrap().url.clone();
    for attempt in 0..=3 {
        file.set_len(0).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        control.received.store(0, Ordering::Relaxed);
        let result = async {
            let mut response = send(
                client.get(&url).header(ACCEPT_ENCODING, "identity"),
                control,
            )
            .await?;
            identity(&response)?;
            if response.status() != StatusCode::OK {
                return Err(format!(
                    "Server returned HTTP {}",
                    response.status().as_u16()
                ));
            }
            let total = response.content_length();
            if total.is_some_and(|n| n > MAX_FILE) {
                return Err("File exceeds 16 TiB".into());
            }
            control.total.store(total.unwrap_or(0), Ordering::Relaxed);
            let mut count = 0u64;
            let mut digest = Sha256::new();
            while let Some(chunk) = bytes(&mut response, control).await? {
                for slice in chunk.chunks(32768) {
                    count = count
                        .checked_add(slice.len() as u64)
                        .ok_or("Transfer size overflow")?;
                    if count > MAX_FILE || total.is_some_and(|total| count > total) {
                        return Err("Response exceeded declared size".into());
                    }
                    throttle(control, rate, slice.len() as u64).await?;
                    file.write_all(slice).map_err(|e| e.to_string())?;
                    digest.update(slice);
                    control.received.store(count, Ordering::Relaxed);
                }
            }
            if total.is_some_and(|total| count != total) {
                return Err("Truncated download".into());
            }
            file.sync_all().map_err(|e| e.to_string())?;
            // Mark complete only in memory: a restarted unvalidated stream starts at zero.
            {
                let mut saved = manifest.lock().unwrap();
                saved.total = Some(count);
                saved.parts[0].end = Some(count);
                saved.parts[0].committed = count;
                saved.parts[0].digest = hex(&digest);
            }
            Ok::<(), String>(())
        }
        .await;
        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                if control.stop.load(Ordering::Relaxed) || attempt == 3 {
                    return Err(error);
                }
                status(control, "Retrying single connection from start");
                tokio::select! { _=stopped(control)=>return Err("Paused".into()), _=tokio::time::sleep(Duration::from_secs(2<<attempt))=>{} }
            }
        }
    }
    let _ = stage;
    Err("Transfer could not finish".into())
}
fn publish(
    job: &Job,
    stage: &Path,
    manifest: &Mutex<Manifest>,
    control: &Control,
) -> Result<String, String> {
    let complete = stage.join("complete.part");
    if complete.symlink_metadata().is_ok() {
        fs::remove_file(&complete).map_err(|e| e.to_string())?;
    }
    let mut output = open(&complete, true)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 32768];
    let saved = manifest.lock().unwrap().clone();
    let mut count = 0;
    for (index, part) in saved.parts.iter().enumerate() {
        let mut input = open(&stage.join(format!("segment-{index}.part")), false)?;
        if input.metadata().map_err(|e| e.to_string())?.len() != part.committed
            || part.end != Some(part.start + part.committed)
        {
            return Err("Segment size verification failed; final file was not published".into());
        }
        let mut part_digest = Sha256::new();
        let mut left = part.committed;
        while left > 0 {
            if control.stop.load(Ordering::Relaxed) {
                return Err("Paused".into());
            }
            let size = buffer.len().min(left as usize);
            let n = input.read(&mut buffer[..size]).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Segment truncated during assembly".into());
            }
            output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            digest.update(&buffer[..n]);
            part_digest.update(&buffer[..n]);
            left -= n as u64;
            count += n as u64;
        }
        if hex(&part_digest) != part.digest {
            return Err("Segment verification failed; final file was not published".into());
        }
    }
    if saved.total != Some(count) {
        return Err("Final size does not match the transfer".into());
    }
    let actual = hex(&digest);
    if job
        .sha256
        .as_ref()
        .is_some_and(|expected| expected != &actual)
    {
        return Err(
            "Verification failed: publisher SHA-256 mismatch. Final file was not published".into(),
        );
    }
    output.sync_all().map_err(|e| e.to_string())?;
    if hex(&hash(&mut output, count, Some(control))?) != actual {
        return Err("Verification failed: assembled file did not pass read-back SHA-256".into());
    }
    crate::profile::write_record(
        &stage.join("receipt.json"),
        &Receipt {
            destination: job.destination.clone(),
            digest: actual.clone(),
            size: count,
        },
    )?;
    fs::hard_link(&complete, &job.destination)
        .map_err(|e| format!("Cannot publish without overwriting: {e}"))?;
    File::open(job.destination.parent().unwrap())
        .and_then(|directory| directory.sync_all())
        .map_err(|e| format!("File linked but directory sync failed: {e}"))?;
    // Remove only our known staging files; never recursively delete a directory.
    for index in 0..saved.parts.len() {
        let _ = fs::remove_file(stage.join(format!("segment-{index}.part")));
    }
    let _ = fs::remove_file(complete);
    let _ = fs::remove_file(stage.join("manifest.json"));
    // The locked inode remains open until run() exits.
    // Keep a small receipt and the stable lock inode for recovery if UI state
    // could not be persisted before a crash. Segment/assembly data are gone.
    Ok(actual)
}
