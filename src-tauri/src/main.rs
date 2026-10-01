// リリースビルドで余分なコンソールウィンドウを出さない (Windows)。削除しないこと。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    vrcinvitetool_lib::run();
}
