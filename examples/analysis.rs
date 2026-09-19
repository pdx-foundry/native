use pdx_native::{Native, OpenRequest};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let installation_hint = std::env::args_os()
        .nth(1)
        .ok_or("supply an executable or installation path")?
        .into();
    let native = Native::open(OpenRequest { installation_hint })?;
    let result = native.analysis()?.decode_control()?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
