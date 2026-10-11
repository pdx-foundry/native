//! The reader of a modifier block's inherited `name` key. The base member passes the reader to
//! the read slot of the name object at a fixed owner offset. The object's class varies, so the
//! key joins only when every class that the object can have forwards the reader, and nothing
//! else, to one shared reader.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::{
    declarations::{Function, number},
    decode::decode_arm64,
    discovery::Symbol,
    fields::{ReaderJoin, Value},
    readers,
    stop::Unresolved,
};

/// The join of the read slot that every implementation in `implementations` holds.
pub(crate) fn reader(
    implementations: &Result<Vec<Function>, Unresolved>,
    symbols: &[Symbol],
) -> ReaderJoin {
    match implementations {
        Ok(functions) => shared_forward(functions, symbols).unwrap_or_else(ReaderJoin::Missing),
        Err(stop) => ReaderJoin::Missing(stop.clone()),
    }
}

fn shared_forward(
    implementations: &[Function],
    symbols: &[Symbol],
) -> Result<ReaderJoin, Unresolved> {
    let stop = || Unresolved::new("modifier-name-reader");
    let targets: BTreeSet<_> = implementations
        .iter()
        .map(|function| forwarded_target(function).ok_or_else(stop))
        .collect::<Result<_, _>>()?;
    if targets.len() != 1 {
        return Err(stop());
    }

    let target = targets.first().copied().ok_or_else(stop)?;
    let callee = symbols
        .iter()
        .find(|symbol| symbol.address == target)
        .map(|symbol| symbol.name.clone())
        .ok_or_else(stop)?;
    let arguments = BTreeMap::from([("x0".to_owned(), Value::Reader(0))]);
    if !readers::arguments_join(&callee, &arguments, false, None) {
        return Err(stop());
    }

    Ok(ReaderJoin::Joined {
        callee,
        arguments,
        tail: true,
    })
}

/// The target of a body that is exactly `mov x0,x1; b TARGET`: it passes its reader argument
/// as the only argument and returns through the call.
fn forwarded_target(function: &Function) -> Option<u64> {
    let rows = decode_arm64(&function.code, function.address).ok()?;
    let [forward, call] = rows.as_slice() else {
        return None;
    };

    (forward.operation == "mov" && forward.operands == "x0,x1" && call.operation == "b")
        .then(|| number(&call.operands))
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::arm64;

    fn symbols() -> Vec<Symbol> {
        [
            (0x9000, readers::SKIP),
            (0x9100, "CReader::Read(bool&)"),
            (0x9200, "CReader::Read(CString&, bool)"),
        ]
        .into_iter()
        .map(|(address, name)| Symbol {
            address,
            name: name.into(),
        })
        .collect()
    }

    fn forward(address: u64, target: usize) -> Function {
        Function {
            address,
            code: arm64!(at address; mov x0, x1; b extern target),
        }
    }

    #[test]
    fn implementations_that_all_skip_the_value_join_the_skip_reader() {
        let implementations = Ok(vec![forward(0x1000, 0x9000), forward(0x2000, 0x9000)]);

        let join = reader(&implementations, &symbols());

        assert!(
            matches!(&join, ReaderJoin::Joined { callee, .. } if callee == readers::SKIP),
            "{join:?}"
        );
    }

    #[test]
    fn a_differing_or_incomplete_implementation_keeps_the_stop() {
        let receiver_overwritten = Function {
            address: 0x3000,
            code: arm64!(at 0x3000; mov x0, x2; b extern 0x9000),
        };
        for implementations in [
            vec![forward(0x1000, 0x9000), forward(0x2000, 0x9200)],
            vec![forward(0x1000, 0x9000), receiver_overwritten],
            vec![forward(0x1000, 0x9100), forward(0x2000, 0x9100)],
            vec![],
        ] {
            let join = reader(&Ok(implementations), &symbols());

            assert!(matches!(join, ReaderJoin::Missing(_)), "{join:?}");
        }

        let unlisted = Err(Unresolved::new("modifier-name-implementations"));
        assert!(matches!(
            reader(&unlisted, &symbols()),
            ReaderJoin::Missing(stop) if stop.reason == "modifier-name-implementations"
        ));
    }
}
