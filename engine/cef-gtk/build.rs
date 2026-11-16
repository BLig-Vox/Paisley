fn main(){
 println!("cargo:rerun-if-changed=proto/ipc.proto");
 prost_build::compile_protos(&["proto/ipc.proto"],&["proto/"]).unwrap();
}
