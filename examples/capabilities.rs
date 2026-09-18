use pdx_native::{CapabilityRequest, Engine, OpenRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let installation_hint = std::env::args_os()
        .nth(1)
        .ok_or("supply an installation or executable path")?
        .into();
    let context = Engine::open(OpenRequest { installation_hint })?;
    println!(
        "{}",
        serde_json::to_string_pretty(&context.capability(&CapabilityRequest::default()))?
    );
    Ok(())
}
