use std::collections::BTreeMap;
use std::ops::DerefMut;
use std::sync::Mutex;

use crate::evt::Event;

use opentelemetry::{
    metrics::{Meter, ObservableGauge, Unit},
    Context,
};

pub struct RawMetrics {
    raw: Vec<u8>,
}

impl RawMetrics {
    fn to_items(&self) -> Vec<Result<RawMetricsItem, Event>> {
        self.raw
            .split(|u: &u8| b'\n'.eq(u))
            .map(RawMetricsItem::try_from)
            .collect()
    }

    pub fn new_metrics_source<S>(raw_source: S) -> impl Fn() -> Vec<Result<RawMetricsItem, Event>>
    where
        S: Fn() -> Result<Self, Event>,
    {
        move || match raw_source() {
            Ok(r) => r.to_items(),
            Err(e) => vec![Err(e)],
        }
    }
}

impl From<Vec<u8>> for RawMetrics {
    fn from(raw: Vec<u8>) -> Self {
        Self { raw }
    }
}

pub struct RawMetricsItem {
    key: Vec<u8>,
    val: Vec<u8>,
}

impl RawMetricsItem {
    fn new(key: &[u8], val: &[u8]) -> Self {
        Self {
            key: key.into(),
            val: val.into(),
        }
    }

    fn as_key(&self) -> &[u8] {
        &self.key
    }
    fn as_val(&self) -> &[u8] {
        &self.val
    }
}

impl TryFrom<&[u8]> for RawMetricsItem {
    type Error = Event;
    fn try_from(line: &[u8]) -> Result<Self, Self::Error> {
        let mut splited = line.splitn(2, |u: &u8| b':'.eq(u));
        let key: &[u8] = splited
            .next()
            .ok_or_else(|| Event::UnexpectedError(String::from("No key found")))?;
        let val: &[u8] = splited
            .next()
            .ok_or_else(|| Event::UnexpectedError(String::from("No val found")))?;
        Ok(Self::new(key, val))
    }
}

#[derive(serde::Deserialize)]
pub struct RawUnit {
    raw: String,
}

impl From<String> for RawUnit {
    fn from(raw: String) -> Self {
        Self { raw }
    }
}

impl From<RawUnit> for Unit {
    fn from(r: RawUnit) -> Self {
        Self::new(r.raw)
    }
}

#[derive(serde::Deserialize)]
pub struct RawInstrumentBuilder {
    key: String,
    description: Option<String>,
    unit: Option<String>,
    gauge_type: String,
}

impl RawInstrumentBuilder {
    pub fn as_key(&self) -> &str {
        self.key.as_str()
    }

    pub fn as_desc(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn as_unit(&self) -> Option<&str> {
        self.unit.as_deref()
    }

    pub fn as_type(&self) -> &str {
        self.gauge_type.as_str()
    }
}

#[derive(serde::Deserialize)]
pub struct RawInstrumentBuilders {
    builders: Vec<RawInstrumentBuilder>,
}

impl RawInstrumentBuilders {
    pub fn into_builders(self) -> impl Iterator<Item = RawInstrumentBuilder> {
        self.builders.into_iter()
    }

    #[cfg(feature = "config-toml")]
    pub fn from_toml_str(s: &str) -> Result<Self, Event> {
        Self::from_toml_bytes(s.as_bytes())
    }

    #[cfg(feature = "config-toml")]
    pub fn from_toml_bytes(s: &[u8]) -> Result<Self, Event> {
        toml::from_slice(s)
            .map_err(|e| Event::UnexpectedError(format!("Invalid metrics config: {}", e)))
    }
}

#[derive(Default)]
pub struct Instruments {
    gauge_i64: BTreeMap<Vec<u8>, ObservableGauge<i64>>,
    gauge_f64: BTreeMap<Vec<u8>, ObservableGauge<f64>>,
    gauge_u64: BTreeMap<Vec<u8>, ObservableGauge<u64>>,
}

impl Instruments {
    pub fn new<I>(m: &Meter, mut builders: I) -> Result<Self, Event>
    where
        I: Iterator<Item = RawInstrumentBuilder>,
    {
        builders.try_fold(Self::default(), |i, builder| i.add(m, builder))
    }

    fn add(mut self, m: &Meter, b: RawInstrumentBuilder) -> Result<Self, Event> {
        let key: String = b.key;
        let k: Vec<u8> = key.as_bytes().into();
        let odesc: Option<String> = b.description;
        let ounit: Option<Unit> = b.unit.map(RawUnit::from).map(RawUnit::into);

        match b.gauge_type.as_str() {
            "i64" => {
                let gauge = m.i64_observable_gauge(key);

                let gauge = match odesc {
                    Some(desc) => gauge.with_description(desc),
                    None => gauge,
                };

                let gauge = match ounit {
                    Some(unit) => gauge.with_unit(unit),
                    None => gauge,
                };

                let o: ObservableGauge<i64> = gauge
                    .try_init()
                    .map_err(|e| Event::UnexpectedError(format!("Unable to build gauge: {}", e)))?;
                self.gauge_i64.insert(k, o);
                Ok(self)
            }
            "u64" => {
                let gauge = m.u64_observable_gauge(key);

                let gauge = match odesc {
                    Some(desc) => gauge.with_description(desc),
                    None => gauge,
                };

                let gauge = match ounit {
                    Some(unit) => gauge.with_unit(unit),
                    None => gauge,
                };

                let o: ObservableGauge<u64> = gauge
                    .try_init()
                    .map_err(|e| Event::UnexpectedError(format!("Unable to build gauge: {}", e)))?;
                self.gauge_u64.insert(k, o);
                Ok(self)
            }
            "f64" => {
                let gauge = m.f64_observable_gauge(key);

                let gauge = match odesc {
                    Some(desc) => gauge.with_description(desc),
                    None => gauge,
                };

                let gauge = match ounit {
                    Some(unit) => gauge.with_unit(unit),
                    None => gauge,
                };

                let o: ObservableGauge<f64> = gauge
                    .try_init()
                    .map_err(|e| Event::UnexpectedError(format!("Unable to build gauge: {}", e)))?;
                self.gauge_f64.insert(k, o);
                Ok(self)
            }
            _ => Ok(self),
        }
    }
    fn observe_i64(ctx: &Context, g: &ObservableGauge<i64>, val: &[u8]) -> Result<(), Event> {
        let s: &str = std::str::from_utf8(val)
            .map_err(|e| Event::UnexpectedError(format!("Invalid bytes: {}", e)))?
            .trim_end();
        let i: i64 = str::parse(s)
            .map_err(|e| Event::UnexpectedError(format!("Invalid integer({}): {}", s, e)))?;
        g.observe(ctx, i, &[]);
        Ok(())
    }

    fn observe_f64(ctx: &Context, g: &ObservableGauge<f64>, val: &[u8]) -> Result<(), Event> {
        let s: &str = std::str::from_utf8(val)
            .map_err(|e| Event::UnexpectedError(format!("Invalid bytes: {}", e)))?
            .trim_end();
        let f: f64 = str::parse(s)
            .map_err(|e| Event::UnexpectedError(format!("Invalid float({}): {}", s, e)))?;
        g.observe(ctx, f, &[]);
        Ok(())
    }

    fn observe_u64(ctx: &Context, g: &ObservableGauge<u64>, val: &[u8]) -> Result<(), Event> {
        let s: &str = std::str::from_utf8(val)
            .map_err(|e| Event::UnexpectedError(format!("Invalid bytes: {}", e)))?
            .trim_end();
        let u: u64 = str::parse(s)
            .map_err(|e| Event::UnexpectedError(format!("Invalid integer({}): {}", s, e)))?;
        g.observe(ctx, u, &[]);
        Ok(())
    }

    fn observe(&self, c: &Context, key: &[u8], val: &[u8]) -> Result<(), Event> {
        self.gauge_i64
            .get(key)
            .iter()
            .try_for_each(|o| Self::observe_i64(c, o, val))?;
        self.gauge_u64
            .get(key)
            .iter()
            .try_for_each(|o| Self::observe_u64(c, o, val))?;
        self.gauge_f64
            .get(key)
            .iter()
            .try_for_each(|o| Self::observe_f64(c, o, val))?;
        Ok(())
    }

    pub fn observe_all<I, E>(&self, c: &Context, items: I, handler: &E)
    where
        I: Iterator<Item = Result<RawMetricsItem, Event>>,
        E: Fn(Event),
    {
        items.flat_map(|r| r.ok()).for_each(|i: RawMetricsItem| {
            let key: &[u8] = i.as_key();
            let val: &[u8] = i.as_val();
            match self.observe(c, key, val) {
                Ok(_) => {}
                Err(e) => handler(e),
            }
        });
    }
}

pub fn raw_metrics_source_from_mut<F>(f: F) -> impl Fn() -> Result<RawMetrics, Event>
where
    F: FnMut() -> Result<RawMetrics, Event>,
{
    let lf: Mutex<F> = Mutex::new(f);
    move || match lf.try_lock() {
        Err(e) => Err(Event::UnexpectedError(format!("Unable to lock: {}", e))),
        Ok(mut gf) => {
            let mf: &mut F = gf.deref_mut();
            mf()
        }
    }
}

pub fn callback_new<S, E>(metrics_source: S, handler: E, i: Instruments) -> impl Fn(&Context)
where
    S: Fn() -> Vec<Result<RawMetricsItem, Event>>,
    E: Fn(Event),
{
    move |c: &Context| {
        let v: Vec<_> = metrics_source();
        i.observe_all(c, v.into_iter(), &handler)
    }
}
