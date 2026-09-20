// 避免在 Windows release 构建时弹出额外的控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    jlu_oa_notifier_lib::run()
}
