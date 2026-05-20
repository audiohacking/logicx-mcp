use logicx_plugin::Plugin;

fn main() {
    if std::env::args().any(|a| a == "--control-bridge") {
        logicx_control::bridge::run_server();
        return;
    }
    truce_standalone::run::<Plugin>();
}
