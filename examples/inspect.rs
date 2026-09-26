//! Look inside any ARM64 executable, catalogued or not: symbols, a function's code, its direct
//! callers, the code that forms a string's address, and fixed-up slots such as a vtable's. On a
//! catalogued build, `--registry-fields` runs the registry field method and shows where each
//! token path stopped; `--trigger-grammar` and `--effect-grammar` do the same for one command's
//! child grammar. With `--trace`, an obstruction at an unknown value also lists where that value
//! stopped being known. No game starts. Addresses in this output are for development only.
//!
//! The image is `--image PATH`, or `STELLARIS_PATH` when that is absent. A directory resolves to
//! its executable the way `Native::open` resolves an installation.
use pdx_native::internals::command_grammar_stops::{self, GrammarResult};
use pdx_native::internals::inspect::{Image, read_image};
use pdx_native::internals::registry_field_stops::{
    self, CAUSE_LIMIT, Cause, Obstacle, PathOutcome, ReaderJoin, RegistryFieldResult, TokenPath,
    Unresolved,
};
use pdx_native::internals::trace_causes;
use pdx_native::{DeclarationKind, Native};

const USAGE: &str = "usage: inspect [--image PATH] \
    (--symbols TEXT | --function NAME|0xADDRESS [--limit BYTES] | --callers NAME|0xADDRESS \
    | --strings TEXT | --slots NAME|0xADDRESS [--count N] | --registry-fields DIRECTORY \
    | --trigger-grammar NAME | --effect-grammar NAME) [--trace]";

/// The instructions shown before each stop.
const TRAIL: usize = 8;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = Arguments::parse(
        std::env::args().skip(1),
        std::env::var("STELLARIS_PATH").ok(),
    )?;
    let bytes = read_image(arguments.image.as_ref())?;
    let image = Image::read(&bytes)?;

    println!("{}", image.identity()?);
    match image.pointer_resolution() {
        Ok(()) => println!("pointer resolution: available"),
        Err(diagnostic) => println!("pointer resolution unavailable: {diagnostic}"),
    }
    println!();

    match arguments.command {
        Command::Symbols(text) => {
            for (address, name) in image.symbols(&text) {
                println!("{address:#x}  {name}");
            }
        }
        Command::Function(query) => {
            let start = image.address(&query)?;
            print!("{}", image.disassemble(start, arguments.limit)?);
        }
        Command::Callers(query) => {
            println!("direct bl and b only; calls through a register or a vtable are not found");
            for caller in image.callers(image.address(&query)?) {
                println!("{caller}");
            }
        }
        Command::Strings(text) => {
            println!("adr, or adrp then add in one function with no branch or write between them");
            for reference in image.string_references(&text) {
                println!("{reference}");
            }
        }
        Command::Slots(query) => {
            for slot in image.slots(image.address(&query)?, arguments.count)? {
                println!("{slot}");
            }
        }
        Command::RegistryFields(registry) => {
            let native = Native::open(&arguments.image)?;
            let run = traced_if(arguments.trace, || {
                registry_field_stops::run(&native, &registry)
            })?;
            print_registry_fields(&image, &registry, &run.result, arguments.trace);
        }
        Command::Grammar(kind, name) => {
            let native = Native::open(&arguments.image)?;
            let run = traced_if(arguments.trace, || {
                command_grammar_stops::run(&native, kind, &name)
            })?;
            println!("{kind:?} {name}: {:?}", run.answer.completeness);
            match &run.result {
                Ok(result) => print_grammar(&image, result, arguments.trace),
                Err(unresolved) => {
                    println!("the receiver join stopped");
                    print_stop(&image, unresolved, arguments.trace);
                }
            }
        }
    }

    Ok(())
}

/// Every stopped token path with its stop and the instructions before it, then every gap.
fn print_registry_fields(
    image: &Image,
    registry: &str,
    result: &RegistryFieldResult,
    traced: bool,
) {
    for collection in &result.collections {
        println!(
            "nested token {} at +{:#x}: {} ({} fields)",
            collection.token,
            collection.offset,
            collection.class,
            collection.fields.fields.len()
        );
        println!("array data offset: {:?}", collection.data_offset);
    }
    for selection in &result.uses {
        println!("use {:?}", selection);
    }
    let stopped = stopped_paths(&result.paths);
    println!(
        "registry {registry}: {} token paths; {} stopped; {} gaps",
        result.paths.len(),
        stopped.len(),
        result.gaps.len()
    );

    for (index, path, unresolved) in stopped {
        let field = result
            .fields
            .iter()
            .find(|field| field.paths.contains(&index))
            .map_or("no field", |field| field.name.as_str());
        let [low, high] = path.domain;
        println!("\npath {index}: tokens {low} to {high}, {field}");
        print_stop(image, unresolved, traced);
        print_trail(image, path);
    }

    println!("\ngaps; a path's stop is shown with its path above");
    for gap in &result.gaps {
        let path = gap
            .path
            .map_or(String::new(), |path| format!(" (path {path})"));
        println!("  {:?}{path}: {}", gap.kind, gap.reason);
        if let (Some(stop), None) = (gap.stop, gap.path) {
            println!("{}", image.place_stop(stop));
        }
    }
}

/// The child paths that stopped, with the grammar's own stops and its numeric child grammar.
fn print_grammar(image: &Image, result: &GrammarResult, traced: bool) {
    let paths = &result.fields.paths;
    let stopped = stopped_paths(paths);
    println!(
        "reader {} with members {}: {} child paths; {} stopped; {} stops",
        result.reader_name,
        result.member_name,
        paths.len(),
        stopped.len(),
        result.stops.len()
    );

    for (index, path, unresolved) in stopped {
        let [low, high] = path.domain;
        println!("\npath {index}: tokens {low} to {high}");
        print_stop(image, unresolved, traced);
        print_trail(image, path);
    }

    println!("\nstops; a path's stop is shown with its path above");
    for unresolved in &result.stops {
        print_stop(image, unresolved, traced);
    }

    if let Some(numeric) = &result.numeric {
        println!("\nnumeric child grammar");
        print_grammar(image, numeric, traced);
    }
}

fn stopped_paths(paths: &[TokenPath]) -> Vec<(usize, &TokenPath, &Unresolved)> {
    paths
        .iter()
        .enumerate()
        .filter_map(|(index, path)| match &path.outcome {
            PathOutcome::Gap(unresolved) | PathOutcome::Reader(ReaderJoin::Missing(unresolved)) => {
                Some((index, path, unresolved))
            }
            PathOutcome::Rejected | PathOutcome::Reader(ReaderJoin::Joined { .. }) => None,
        })
        .collect()
}

fn print_trail(image: &Image, path: &TokenPath) {
    let trail = &path.instructions[path.instructions.len().saturating_sub(TRAIL)..];
    println!("  the path's last instructions:");
    for &address in trail {
        print_instruction(image, address, "    ");
    }
}

fn print_instruction(image: &Image, address: u64, indent: &str) {
    match image.disassemble(address, 4) {
        Ok(listing) => listing
            .rows
            .iter()
            .for_each(|row| println!("{indent}{row}")),
        Err(error) => println!("{indent}{address:#x}  {error}"),
    }
}

/// The obstruction, and with `traced`, where the value that it needed stopped being known.
fn print_stop(image: &Image, unresolved: &Unresolved, traced: bool) {
    match unresolved.stop {
        Some(stop) => println!("{}: {}", unresolved.reason, image.place_stop(stop)),
        None => println!("{}: no instruction", unresolved.reason),
    }

    match &unresolved.trace {
        Some(trace) => {
            let causes: Vec<_> = trace.causes().collect();
            match causes.len() {
                0 => println!("  no recorded cause"),
                1 => println!("  the value stopped being known here:"),
                _ => println!("  the value stopped being known at one or more of these:"),
            }
            for cause in causes {
                print_cause(image, cause);
            }
            if trace.truncated {
                println!("  more causes were dropped; a trace keeps at most {CAUSE_LIMIT}");
            }
            if trace.unrecorded {
                println!(
                    "  part of the value has no recorded cause: unknown when the walk began, \
                     in memory the path never wrote, or in a vector register"
                );
            }
        }
        None if traced && is_unknown_value(unresolved) => {
            println!("  no causes: this walk does not trace them");
        }
        None => {}
    }
}

fn is_unknown_value(unresolved: &Unresolved) -> bool {
    matches!(
        unresolved.stop.map(|stop| stop.obstacle),
        Some(Obstacle::Unknown(_))
    )
}

fn print_cause(image: &Image, cause: Cause) {
    println!(
        "    {} at {}; entered at {:#x} {}",
        cause.kind,
        image.place(cause.instruction),
        cause.entry,
        image.place(cause.entry)
    );
    print_instruction(image, cause.instruction, "      ");
}

enum Command {
    Symbols(String),
    Function(String),
    Callers(String),
    Strings(String),
    Slots(String),
    RegistryFields(String),
    Grammar(DeclarationKind, String),
}

struct Arguments {
    image: String,
    command: Command,
    limit: u64,
    count: usize,
    trace: bool,
}

impl Arguments {
    /// `default_image` is used when `--image` is absent.
    fn parse(
        mut arguments: impl Iterator<Item = String>,
        default_image: Option<String>,
    ) -> Result<Self, String> {
        let mut image = default_image;
        let mut command = None;
        let mut limit = 64 * 1024;
        let mut count = 16;
        let mut trace = false;

        while let Some(flag) = arguments.next() {
            if flag == "--trace" {
                trace = true;
                continue;
            }
            let value = arguments.next().ok_or(USAGE)?;
            match flag.as_str() {
                "--image" => image = Some(value),
                "--symbols" => command = Some(Command::Symbols(value)),
                "--function" => command = Some(Command::Function(value)),
                "--callers" => command = Some(Command::Callers(value)),
                "--strings" => command = Some(Command::Strings(value)),
                "--slots" => command = Some(Command::Slots(value)),
                "--registry-fields" => command = Some(Command::RegistryFields(value)),
                "--trigger-grammar" => {
                    command = Some(Command::Grammar(DeclarationKind::Trigger, value));
                }
                "--effect-grammar" => {
                    command = Some(Command::Grammar(DeclarationKind::Effect, value));
                }
                "--limit" => limit = value.parse().map_err(|_| USAGE)?,
                "--count" => count = value.parse().map_err(|_| USAGE)?,
                _ => return Err(USAGE.into()),
            }
        }

        Ok(Self {
            image: image.ok_or(USAGE)?,
            command: command.ok_or(USAGE)?,
            limit,
            count,
            trace,
        })
    }
}

/// Run a method, with cause tracing when `trace` is set.
fn traced_if<T>(trace: bool, method: impl FnOnce() -> T) -> T {
    if trace {
        trace_causes(method)
    } else {
        method()
    }
}
