//! File adapter to the core FixedClient. No algorithm execution or model calls.
use jpp_core::{
    effects::{CalibStore, FixedClient},
    value::{Answer, Mat, Op, Question, State},
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Fixture {
    pub description: String,
    pub calibrations: Vec<Calibration>,
    pub observations: Vec<Observation>,
}

#[derive(Deserialize)]
pub struct Calibration {
    pub key: String,
    pub hi: f64,
    pub lo: f64,
    pub n: u64,
    pub status: String,
}

#[derive(Deserialize)]
pub struct Observation {
    pub on: Vec<Value>,
    #[serde(default)]
    pub ctx: Vec<Value>,
    #[serde(default)]
    pub r#ref: Vec<Value>,
    #[serde(default)]
    pub over: Vec<Value>,
    pub op: String,
    pub text: String,
    pub calib: String,
    #[serde(default)]
    pub scale: Vec<String>,
    pub answer: Answer,
}

impl Fixture {
    pub fn build(&self) -> Result<(FixedClient, CalibStore), String> {
        let mut client = FixedClient::new();
        let mut calibrations = CalibStore::new();
        for c in &self.calibrations {
            calibrations.put(&c.key, c.hi, c.lo, c.n, &c.status)?;
        }
        let mats = |items: &[Value]| items.iter().cloned().map(Mat::literal).collect();
        for o in &self.observations {
            let state = State::new(
                mats(&o.on),
                mats(&o.ctx),
                mats(&o.r#ref),
                mats(&o.over),
                false,
            );
            let op = match o.op.as_str() {
                "test" => Op::Test,
                "select" => Op::Select,
                "measure" => Op::Measure,
                other => return Err(format!("unknown fixture question kind '{other}'")),
            };
            let question = Question::new(op, &o.text, &o.calib, o.scale.clone());
            client.observe(&state, &question, o.answer.clone());
        }
        Ok((client, calibrations))
    }
}
