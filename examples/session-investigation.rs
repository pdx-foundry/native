//! Maintainer controls for the same Game lifecycle used by production consumers.
#[path = "support/session.rs"]
mod session;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    session::run()
}
