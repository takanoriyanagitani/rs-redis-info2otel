#[cfg(feature = "config-toml")]
mod config_toml {
    use rs_redis_info2otel::metrics::{RawInstrumentBuilder, RawInstrumentBuilders};

    #[test]
    fn test_toml() {
        let source: &[u8] = br#"

            [[builders]]
            key = "connected_clients"
            description = "Number of connected clients."
            unit = ""
            gauge_type = "u64"

            [[builders]]
            key = "cluster_connections"
            description = "Number of cluster connections."
            unit = ""
            gauge_type = "u64"

            [[builders]]
            key = "used_cpu_sys"
            description = "System CPU Seconds used by the redis server."
            unit = ""
            gauge_type = "f64"

        "#;

        let builders: RawInstrumentBuilders =
            RawInstrumentBuilders::from_toml_bytes(source).unwrap();
        let mut i = builders.into_builders();

        let clients: RawInstrumentBuilder = i.next().unwrap();
        assert_eq!(clients.as_key(), "connected_clients");
        assert_eq!(clients.as_desc().unwrap(), "Number of connected clients.");
        assert_eq!(clients.as_unit().unwrap(), "");
        assert_eq!(clients.as_type(), "u64");
    }
}
