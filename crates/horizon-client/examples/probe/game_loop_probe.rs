#![forbid(unsafe_code)]
//! Runs the JSON-lines scenarios of tools/test_game_loop_parity.py through the
//! Rust tick-rate manager.
use horizon_client::{Result, clock::ServerTickRateManager};
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
};

fn fl(v: &Value) -> f32 {
    f32::from_bits(u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn fb(v: f32) -> String {
    format!("{:08x}", if v.is_nan() { 0x7fc0_0000 } else { v.to_bits() })
}
fn b(v: &Value) -> bool {
    v.as_bool().unwrap()
}

fn rate(c: &Value, out: &mut impl Write) -> Result<()> {
    let mut m = ServerTickRateManager::default();
    writeln!(out, "rate")?;
    for op in c["ops"].as_array().unwrap() {
        let name = op[0].as_str().unwrap();
        let mut result = String::new();
        let mut report = None;
        match name {
            "rate" => m.set_tick_rate(fl(&op[1])),
            "freeze" => m.base.set_frozen(b(&op[1])),
            "step" => {
                result = format!(" {}", m.step_game_if_paused(op[1].as_i64().unwrap() as i32))
            }
            "stopstep" => result = format!(" {}", m.stop_stepping()),
            "sprint" => {
                result = format!(
                    " {}",
                    m.request_game_to_sprint(op[1].as_i64().unwrap() as i32)
                )
            }
            "check" => {
                let (run, r) = m.check_should_sprint_this_tick(op[1].as_i64().unwrap());
                result = format!(" {run}");
                report = r;
            }
            "end" => m.end_tick_work(op[1].as_i64().unwrap()),
            "stopsprint" => {
                report = m.stop_sprinting();
                result = format!(" {}", report.is_some());
            }
            "tick" => m.base.tick(),
            _ => panic!("{name}"),
        }
        let tail = report.map_or(String::new(), |r| {
            format!(" report {} {}", r.ticks_per_second, r.milliseconds_per_tick)
        });
        writeln!(
            out,
            "{name}{result} | {} {} {} {} {} {} {} {}{tail}",
            fb(m.base.tickrate()),
            m.base.nanoseconds_per_tick(),
            fb(m.base.milliseconds_per_tick()),
            m.base.runs_normally(),
            m.base.is_stepping_forward(),
            m.base.frozen_ticks_to_run(),
            m.base.is_frozen(),
            m.is_sprinting()
        )?;
    }
    Ok(())
}

pub fn main() -> Result<()> {
    let args: Vec<_> = crate::args().skip(1).collect();
    let mut out = BufWriter::new(File::create(&args[1])?);
    for line in BufReader::new(File::open(&args[0])?).lines() {
        let c: Value = serde_json::from_str(&line?)?;
        match c["t"].as_str().unwrap() {
            "rate" => rate(&c, &mut out)?,
            other => panic!("{other}"),
        }
    }
    Ok(())
}
