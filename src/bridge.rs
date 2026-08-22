#[cxx_qt::bridge(namespace = "anticaptrad")]
pub mod qobject {
    #[namespace = ""]
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, creator_handle, cxx_name = "creatorHandle")]
        #[qproperty(QString, status)]
        #[qproperty(QString, transport)]
        #[qproperty(QString, stack)]
        #[qproperty(bool, transport_ready, cxx_name = "transportReady")]
        type StudioController = super::StudioControllerRust;

        #[qinvokable]
        #[cxx_name = "probeTransport"]
        fn probe_transport(self: Pin<&mut Self>);

        #[qinvokable]
        #[cxx_name = "resetTransport"]
        fn reset_transport(self: Pin<&mut Self>);
    }
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use crate::core::EXPECTED_CREATOR_HANDLE;
use crate::runtime::RuntimeSupervisor;

pub struct StudioControllerRust {
    creator_handle: QString,
    status: QString,
    transport: QString,
    stack: QString,
    transport_ready: bool,
    runtime: Option<RuntimeSupervisor>,
}

impl Default for StudioControllerRust {
    fn default() -> Self {
        let (runtime, status) = match RuntimeSupervisor::new() {
            Ok(runtime) => (Some(runtime), "Media runtime ready"),
            Err(_) => (None, "Media runtime unavailable"),
        };

        Self {
            creator_handle: QString::from(EXPECTED_CREATOR_HANDLE),
            status: QString::from(status),
            transport: QString::from("Not probed"),
            stack: QString::from(RuntimeSupervisor::stack_summary()),
            transport_ready: false,
            runtime,
        }
    }
}

impl qobject::StudioController {
    pub fn probe_transport(mut self: Pin<&mut Self>) {
        let result = self.as_ref().rust().runtime.as_ref().map_or_else(
            || Err("Tokio media runtime did not start".to_owned()),
            |runtime| {
                runtime
                    .probe_udp_loopback()
                    .map(|peer| format!("UDP loopback verified via {peer}"))
                    .map_err(|error| error.to_string())
            },
        );

        match result {
            Ok(transport) => {
                self.as_mut().set_transport(QString::from(&transport));
                self.as_mut().set_transport_ready(true);
                self.set_status(QString::from("Transport diagnostic passed"));
            }
            Err(error) => {
                self.as_mut().set_transport(QString::from(&error));
                self.as_mut().set_transport_ready(false);
                self.set_status(QString::from("Transport diagnostic failed"));
            }
        }
    }

    pub fn reset_transport(mut self: Pin<&mut Self>) {
        self.as_mut().set_transport(QString::from("Not probed"));
        self.as_mut().set_transport_ready(false);
        self.set_status(QString::from("Media runtime ready"));
    }
}
