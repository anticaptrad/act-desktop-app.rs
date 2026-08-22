use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("com.anticaptrad.studio").qml_file("qml/Main.qml"))
        .files(["src/bridge.rs"])
        .qt_module("Multimedia")
        .qt_module("Network")
        .build();
}
