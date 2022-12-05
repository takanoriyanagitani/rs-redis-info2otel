use std::io::{Read, Write};

use opentelemetry::{
    global,
    metrics::Meter,
    sdk::{
        export::metrics::aggregation,
        metrics::{controllers, processors, selectors},
    },
};

use prometheus::{Encoder, TextEncoder};

use redis::{Client, ConnectionLike};

use rs_redis_info2otel::evt::Event;
use rs_redis_info2otel::metrics::{
    callback_new, raw_metrics_source_from_mut, Instruments, RawInstrumentBuilders, RawMetrics,
    RawMetricsItem,
};

fn redis_con_new(addr: &str) -> Result<Client, Event> {
    Client::open(addr)
        .map_err(|e| Event::UnexpectedError(format!("Unable to connect to redis: {}", e)))
}

fn redis_info_new_raw<C>(mut conn: C) -> impl FnMut() -> Result<Vec<u8>, Event>
where
    C: ConnectionLike,
{
    move || {
        redis::cmd("INFO")
            .query(&mut conn)
            .map_err(|e| Event::UnexpectedError(format!("Unable to get info: {}", e)))
    }
}

fn redis_metrics_new<C>(conn: C) -> impl FnMut() -> Result<RawMetrics, Event>
where
    C: ConnectionLike,
{
    let mut f = redis_info_new_raw(conn);
    move || f().map(RawMetrics::from)
}

fn redis_metrics_source_new<C>(conn: C) -> impl Fn() -> Vec<Result<RawMetricsItem, Event>>
where
    C: ConnectionLike,
{
    let f = raw_metrics_source_from_mut(redis_metrics_new(conn));
    RawMetrics::new_metrics_source(f)
}

fn init_metrics_prometheus() {
    let controller = controllers::basic(
        processors::factory(
            selectors::simple::histogram([-0.5, 1.0]),
            aggregation::cumulative_temporality_selector(),
        )
        .with_memory(true),
    )
    .build();

    opentelemetry_prometheus::exporter(controller)
        .with_registry(prometheus::default_registry().clone())
        .init();
}

fn main() {
    init_metrics_prometheus();

    let mut toml_source: Vec<u8> = Vec::new();
    let mut stdin = std::io::stdin().lock();
    stdin.read_to_end(&mut toml_source).unwrap();

    let builders: RawInstrumentBuilders =
        RawInstrumentBuilders::from_toml_bytes(&toml_source).unwrap();

    let i = builders.into_builders();

    let m: Meter = global::meter("redis.info2prometheus");
    let gauges: Instruments = Instruments::new(&m, i).unwrap();

    let c: Client = redis_con_new("redis://localhost:6379").unwrap();
    let metrics_source = redis_metrics_source_new(c);

    let error_handler = |e: Event| eprintln!("error: {:#?}", e);

    let callback = callback_new(metrics_source, error_handler, gauges);

    m.register_callback(callback).unwrap();

    let te = TextEncoder::new();
    let metrics_items: Vec<_> = prometheus::default_registry().gather();
    let mut buf: Vec<u8> = Vec::new();
    te.encode(&metrics_items, &mut buf).unwrap();

    let mut o = std::io::stdout().lock();
    o.write_all(&buf).unwrap();
    o.flush().unwrap();
}
