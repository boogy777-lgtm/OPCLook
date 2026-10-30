// Compile the actual adapter and time conversion, not a duplicate mapper.
#[path = "../../../../../crates/backend/src/bff/filetime.rs"]
mod filetime;
#[path = "../../../../../crates/backend/src/bff/softing_facts.rs"]
mod softing_facts;
use softing_facts::{Envelope, FactValue, RawFact, adapt};
use std::{error::Error, fs::File, io::Read};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("missing SDK fixture output")?;
    let mut bytes = Vec::new();
    File::open(path)?.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        return Err("fixture output too large".into());
    }
    let stdout = String::from_utf8(bytes)?;
    let line = stdout
        .lines()
        .find_map(|l| l.strip_prefix("N2_FACT "))
        .ok_or("missing ABI fact")?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() != 11 {
        return Err("invalid fact field count".into());
    }
    let envelope = Envelope {
        mode: parts[0].parse()?,
        transaction: parts[1].parse()?,
        master_quality: parts[2].parse()?,
        master_error: parts[3].parse()?,
    };
    let raw = RawFact {
        client_handle: parts[4].parse()?,
        hresult: parts[5].parse()?,
        filetime_low: parts[6].parse()?,
        filetime_high: parts[7].parse()?,
        i4: parts[8].parse()?,
        quality: parts[9].parse()?,
        vartype: parts[10].parse()?,
    };
    let fact = adapt(envelope, raw).map_err(|_| "invalid adapter fact")?;
    if fact.mode != 0
        || fact.transaction != 42
        || fact.master_quality != 0
        || fact.master_error != 0
        || fact.client_handle == 0
        || fact.hresult != 7
        || fact.filetime_ticks != 1
        || fact.quality != 0xc0
        || fact.vartype != 3
    {
        return Err("incorrect raw fact".into());
    }
    let FactValue::Value(value) = fact.value else {
        return Err("missing value".into());
    };
    if value.value != opclook_contract::model::Value::I64(-42)
        || value.quality.code != 0xc0
        || value
            .timestamp
            .ok_or("missing time")?
            .timestamp_subsec_nanos()
            != 100
    {
        return Err("incorrect contract value".into());
    }
    println!("SDK callback -> C ABI -> Rust contract mapping: PASS");
    Ok(())
}
