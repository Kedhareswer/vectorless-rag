// Prevents console window on Windows — applies to both debug and release builds.
#![windows_subsystem = "windows"]

fn main() {
    vectorless_rag_lib::run()
}
