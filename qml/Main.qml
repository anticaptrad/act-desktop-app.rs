import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import com.anticaptrad.studio 1.0

ApplicationWindow {
    id: window

    width: 1280
    height: 800
    minimumWidth: 980
    minimumHeight: 680
    visible: true
    color: "#090b10"
    title: qsTr("AntiCapTrad Studio")

    readonly property color surface: "#11151d"
    readonly property color raisedSurface: "#181e28"
    readonly property color border: "#2a3240"
    readonly property color primary: "#adff2f"
    readonly property color secondary: "#66d9ff"
    readonly property color textPrimary: "#f2f5f7"
    readonly property color textMuted: "#9ba7b4"

    StudioController {
        id: controller
    }

    component Panel: Rectangle {
        color: window.surface
        radius: 14
        border.color: window.border
        border.width: 1
    }

    component Metric: ColumnLayout {
        property string label
        property string value
        property color accent: window.textPrimary

        spacing: 4
        Label {
            text: parent.label.toUpperCase()
            color: window.textMuted
            font.pixelSize: 10
            font.bold: true
            font.letterSpacing: 1.2
        }
        Label {
            text: parent.value
            color: parent.accent
            font.pixelSize: 15
            font.bold: true
            elide: Text.ElideRight
            Layout.fillWidth: true
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 18

        RowLayout {
            Layout.fillWidth: true
            spacing: 14

            Rectangle {
                width: 42
                height: 42
                radius: 12
                color: window.primary

                Label {
                    anchors.centerIn: parent
                    text: "ACT"
                    color: "#071000"
                    font.pixelSize: 12
                    font.bold: true
                }
            }

            ColumnLayout {
                spacing: 0
                Label {
                    text: qsTr("AntiCapTrad Studio")
                    color: window.textPrimary
                    font.pixelSize: 23
                    font.bold: true
                }
                Label {
                    text: qsTr("Native broadcast control plane")
                    color: window.textMuted
                    font.pixelSize: 12
                }
            }

            Item { Layout.fillWidth: true }

            Rectangle {
                implicitWidth: identityLabel.implicitWidth + 28
                implicitHeight: 34
                radius: 17
                color: "#172113"
                border.color: "#395521"
                Label {
                    id: identityLabel
                    anchors.centerIn: parent
                    text: controller.creatorHandle
                    color: window.primary
                    font.pixelSize: 13
                    font.bold: true
                }
            }
        }

        GridLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            columns: 12
            columnSpacing: 18
            rowSpacing: 18

            Panel {
                Layout.columnSpan: 8
                Layout.rowSpan: 8
                Layout.fillWidth: true
                Layout.fillHeight: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 18
                    spacing: 12

                    RowLayout {
                        Layout.fillWidth: true
                        Label {
                            text: qsTr("PROGRAM MONITOR")
                            color: window.textMuted
                            font.pixelSize: 10
                            font.bold: true
                            font.letterSpacing: 1.3
                        }
                        Item { Layout.fillWidth: true }
                        Rectangle {
                            implicitWidth: liveLabel.implicitWidth + 18
                            implicitHeight: 26
                            radius: 13
                            color: "#28181c"
                            border.color: "#743143"
                            Label {
                                id: liveLabel
                                anchors.centerIn: parent
                                text: qsTr("OFF AIR")
                                color: "#ff7198"
                                font.pixelSize: 10
                                font.bold: true
                            }
                        }
                    }

                    Rectangle {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        radius: 10
                        color: "#050608"
                        border.color: "#202631"

                        Rectangle {
                            anchors.centerIn: parent
                            width: 92
                            height: 92
                            radius: 46
                            color: "#10141b"
                            border.color: window.border

                            Label {
                                anchors.centerIn: parent
                                text: "▶"
                                color: window.secondary
                                font.pixelSize: 28
                            }
                        }

                        Label {
                            anchors.horizontalCenter: parent.horizontalCenter
                            anchors.top: parent.verticalCenter
                            anchors.topMargin: 62
                            text: qsTr("Media surfaces stay native; frames never cross QML as serialized data")
                            color: window.textMuted
                            font.pixelSize: 12
                        }
                    }
                }
            }

            Panel {
                Layout.columnSpan: 4
                Layout.rowSpan: 8
                Layout.fillWidth: true
                Layout.fillHeight: true

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 18
                    spacing: 14

                    Label {
                        text: qsTr("DESTINATIONS")
                        color: window.textMuted
                        font.pixelSize: 10
                        font.bold: true
                        font.letterSpacing: 1.3
                    }

                    Repeater {
                        model: [
                            { name: "YouTube", protocol: "RTMPS + API", color: "#ff5277" },
                            { name: "Twitch", protocol: "RTMPS + EventSub", color: "#b69cff" },
                            { name: "Rumble", protocol: "RTMP + API", color: "#8edb4d" },
                            { name: "StreamYard", protocol: "Studio handoff", color: "#6aa8ff" },
                            { name: "X / Twitter", protocol: "Publisher API", color: "#d7e0e8" }
                        ]

                        delegate: Rectangle {
                            required property var modelData
                            Layout.fillWidth: true
                            implicitHeight: 54
                            radius: 10
                            color: window.raisedSurface
                            border.color: window.border

                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 13
                                anchors.rightMargin: 13
                                spacing: 10
                                Rectangle {
                                    width: 9
                                    height: 9
                                    radius: 5
                                    color: modelData.color
                                }
                                ColumnLayout {
                                    spacing: 1
                                    Label {
                                        text: modelData.name
                                        color: window.textPrimary
                                        font.pixelSize: 13
                                        font.bold: true
                                    }
                                    Label {
                                        text: modelData.protocol
                                        color: window.textMuted
                                        font.pixelSize: 10
                                    }
                                }
                                Item { Layout.fillWidth: true }
                                Label {
                                    text: qsTr("NOT CONNECTED")
                                    color: window.textMuted
                                    font.pixelSize: 9
                                    font.bold: true
                                }
                            }
                        }
                    }

                    Item { Layout.fillHeight: true }
                }
            }

            Panel {
                Layout.columnSpan: 12
                Layout.rowSpan: 4
                Layout.fillWidth: true
                Layout.fillHeight: true

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 18
                    spacing: 26

                    Metric {
                        Layout.preferredWidth: 175
                        label: qsTr("Runtime")
                        value: controller.stack
                        accent: window.secondary
                    }
                    Metric {
                        Layout.preferredWidth: 195
                        label: qsTr("Status")
                        value: controller.status
                    }
                    Metric {
                        Layout.fillWidth: true
                        label: qsTr("UDP diagnostic")
                        value: controller.transport
                        accent: controller.transportReady ? window.primary : window.textMuted
                    }

                    Button {
                        text: qsTr("Reset")
                        flat: true
                        onClicked: controller.resetTransport()
                    }
                    Button {
                        text: qsTr("Probe UDP")
                        font.bold: true
                        onClicked: controller.probeTransport()
                    }
                }
            }
        }
    }
}
