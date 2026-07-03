fn main() {
    // Tell Cargo to recompile this crate whenever any migration file is added,
    // removed, or changed. Without this, `sqlx::migrate!()` silently uses a
    // stale build cache that was compiled before the new migration existed.
    let migrations = std::path::Path::new("migrations");
    println!("cargo:rerun-if-changed={}", migrations.display());
    if let Ok(entries) = std::fs::read_dir(migrations) {
        for entry in entries.flatten() {
            println!("cargo:rerun-if-changed={}", entry.path().display());
        }
    }
}
