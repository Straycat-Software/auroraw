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

    contentItem: ColumnLayout {
        spacing: 10
        Label {
            text: qsTr("Language")
            font.bold: true
        }
        RowLayout {
            spacing: 8
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
            spacing: 8
            Label {
                text: qsTr("Photos of one camera at most this many seconds apart form a series:")
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            SpinBox {
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
    }

    footer: DialogButtonBox {
        AppButton {
            text: qsTr("Close")
            DialogButtonBox.buttonRole: DialogButtonBox.RejectRole
            onClicked: dialog.close()
        }
    }
}
