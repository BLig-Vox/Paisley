//! Provider-neutral stdio adapter. The embedded standard-library Python runner
//! owns a mandatory private namespace and fixed-origin relay; no host listener.
use gtk::glib::ExitCode;

pub fn run_stdio() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !matches!(args.len(), 3 | 5)
        || args[0] != "--mcp"
        || args[1] != "--allow-origin"
        || (args.len() == 5 && args[3] != "--fixture-ca")
    {
        eprintln!(
            "Usage: paisley --mcp --allow-origin HTTPS_ORIGIN|http://127.0.0.1:PORT [--fixture-ca PUBLIC_PEM]"
        );
        return ExitCode::FAILURE;
    }
    let source = include_str!("../assets/mcp.py");
    let quoted = serde_json::to_string(source).expect("serialize embedded adapter");
    let wrapper =
        format!("_PAISLEY_SOURCE = {quoted}\nexec(compile(_PAISLEY_SOURCE, '<paisley-mcp>', 'exec'))");
    let result = std::env::current_exe().and_then(|binary| {
        std::process::Command::new("/usr/bin/python3")
            .args(["-I", "-c", &wrapper])
            .args(&args[1..])
            .env("PAISLEY_BINARY", binary)
            .status()
    });
    match result {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("Could not start isolated MCP adapter: {error}");
            ExitCode::FAILURE
        }
    }
}
