// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The application's settings (D-090): the language, and the series gap (D-101), applied at once and remembered.
AppDialog {
    id: dialog
    required property var launcher
    preferredWidth: 480
    title: qsTr("Settings")
    property alias languageButtons: languages
    property alias gapBox: gapBox
    property alias regroupButton: regroupButton
    property alias similarDistanceBox: similarDistanceBox
    property alias similarMinutesBox: similarMinutesBox
    property alias startupButtons: startup

    contentItem: ColumnLayout {
        spacing: 16
        Label {
            text: qsTr("Startup")
            font.bold: true
        }
        RowLayout {
            spacing: 12
            Repeater {
                id: startup
                model: ["reopen", "list"]
                delegate: AppButton {
                    required property string modelData
                    text: modelData === "reopen" ? qsTr("Reopen the last workspace") : qsTr("Show the list of workspaces")
                    checkable: true
                    autoExclusive: true
                    checked: dialog.launcher.startupBehavior() === modelData
                    onClicked: dialog.launcher.setStartupBehavior(modelData)
                }
            }
        }
        Label {
            text: qsTr("Language")
            font.bold: true
        }
        RowLayout {
            spacing: 12
            Repeater {
                id: languages
                // The model holds no text: it would be rebuilt (and its buttons with it) by every
                // change of language, which Qt 6.4 lays out badly when the dialog is closed.
                model: ["system", "en", "fr"]
                delegate: AppButton {
                    required property string modelData
                    text: modelData === "system" ? qsTr("System")
                          : modelData === "en" ? "English" : "Français"
                    checkable: true
                    autoExclusive: true
                    checked: dialog.launcher.language === modelData
                    onClicked: dialog.launcher.chooseLanguage(modelData)
                }
            }
        }
        Label {
            text: qsTr("Series")
            font.bold: true
            Layout.topMargin: 6
        }
        RowLayout {
            spacing: 12
            Label {
                text: qsTr("Photos of one camera at most this many seconds apart form a series:")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            AppSpinBox {
                id: gapBox
                from: 0
                to: 600
                editable: true
                value: dialog.launcher.seriesGap()
                Accessible.name: qsTr("Series gap in seconds")
                onValueModified: dialog.launcher.setSeriesGap(value)
            }
        }
        AppButton {
            id: regroupButton
            text: qsTr("Regroup the series now")
            Layout.alignment: Qt.AlignLeft
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Forms the series that were made by themselves again with this gap. Series made by hand or resolved stay.")
            onClicked: dialog.launcher.regroupSeries()
        }
        Label {
            text: qsTr("Similar photos")
            font.bold: true
            Layout.topMargin: 6
        }
        RowLayout {
            spacing: 12
            Label {
                text: qsTr("Photos are similar when at most this many of the 64 bits of their pictures' fingerprints differ:")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            AppSpinBox {
                id: similarDistanceBox
                from: 1
                to: 24
                editable: true
                value: dialog.launcher.intOption("similarDistance")
                Accessible.name: qsTr("Similar photos: how many bits may differ")
                onValueModified: dialog.launcher.setIntOption("similarDistance", value)
            }
        }
        RowLayout {
            spacing: 12
            Label {
                text: qsTr("...and they were taken at most this many minutes apart:")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            AppSpinBox {
                id: similarMinutesBox
                from: 1
                to: 10080
                editable: true
                value: dialog.launcher.intOption("similarMinutes")
                Accessible.name: qsTr("Similar photos: how many minutes apart")
                onValueModified: dialog.launcher.setIntOption("similarMinutes", value)
            }
        }
    }

    footer: AppDialogButtonBox {
        AppButton {
            text: qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
