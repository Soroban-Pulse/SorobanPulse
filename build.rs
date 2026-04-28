fn main() {
    if cfg!(all(feature = "otel", feature = "zipkin")) {
        panic!("Features 'otel' and 'zipkin' cannot be enabled together");
    }
}
