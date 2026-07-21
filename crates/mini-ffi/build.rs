fn main() {
    uniffi::generate_scaffolding("src/mini_ffi.udl")
        .expect("mini-ffi UDL must generate valid Rust scaffolding");
}
