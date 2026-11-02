fn main() {
    // SQLx tracks existing migration files; Cargo must also notice new entries.
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=migrations-postgres");
}
