//! ColdStart Windows 桌面入口。
//!
//! 生产构建隐藏控制台窗口，并把控制权交给 Tauri 运行时；MCP 是独立二进制，
//! 不随桌面窗口启动。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    coldstart_desktop_lib::run();
}
