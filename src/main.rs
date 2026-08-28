#[path = "../generated/rust/env.rs"]
mod env;
#[path = "../generated/rust/runtime.rs"]
mod env_runtime;

pub mod bridge;
pub mod core;
pub mod runtime;

use cxx_qt::casting::Upcast;
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QQmlEngine, QUrl};
use std::pin::Pin;
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("act_desktop_app=info")),
        )
        .with_target(false)
        .init();

    let mut application = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(
            "qrc:/qt/qml/com/anticaptrad/studio/qml/Main.qml",
        ));
    }

    if let Some(engine) = engine.as_mut() {
        let qml_engine: Pin<&mut QQmlEngine> = engine.upcast_pin();
        qml_engine
            .on_quit(|_| tracing::info!("QML requested application shutdown"))
            .release();
    }

    if let Some(application) = application.as_mut() {
        application.exec();
    }
}
