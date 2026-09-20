//! List the registries of an installation, or the root fields of one registry. No game starts.
use pdx_native::Native;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let installation = args
        .next()
        .ok_or("usage: registries <installation> [registry]")?;
    let native = Native::open(installation)?;
    match args.next() {
        Some(registry) => println!(
            "{}",
            serde_json::to_string_pretty(&native.registry_fields(&registry)?)?
        ),
        None => println!("{}", serde_json::to_string_pretty(&native.registries()?)?),
    }
    Ok(())
}
