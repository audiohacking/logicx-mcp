//! Companion control server — same process model as logic-pro-mcp's LogicProMCP binary.
//! AU plugins delegate here when running inside AUHostingServiceXPC.

fn main() {
    logicx_control::bridge::run_server();
}
