// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.auroraw.ui

// The Info panel, the inspector's third tab: the technical metadata of the grid's own active photo (its
// cursor, `library.grid.currentIndex` -- always one photo, never the selection, unlike Keywords and
// Metadata: nothing here is edited, so there is no "multiple values" to show). Camera, lens and exposure
// come from the catalogue (`PhotoGrid.technicalInfoAt`, its own columns); the serial number, the 35 mm
// equivalent focal length, orientation and GPS come from the sidecar (`Engine::technical_details_of`,
// merged into the same JSON). A field the photo does not carry is left out rather than shown empty; an
// orientation that is known but "Normal" is still shown. Read-only: EXIF-overlay editing (spec §5.7) is
// a separate, later feature.
Item {
    id: panel
    required property var photoGrid
    required property var library

    readonly property int row: library.grid.currentIndex
    // The active photo's raw values (plain data, reloaded by `refresh()`); `rows` below is a binding
    // over it, not a value assigned imperatively, precisely so that its `qsTr` calls retranslate on a
    // language change the way `MetadataPanel.qml`'s own `fields` binding already does -- an imperative
    // assignment would freeze English (or whatever was current) into `rows` until the photo next changed.
    property var info: ({})
    readonly property var rows: computeRows(panel.info)

    property alias list: list

    // Reloads from the active photo's own current values: called on every cursor move and whenever the
    // grid's data changes (an edit, an undo or a redo, a rescan), `Viewer.qml`'s own `refreshInfo()`.
    function refresh() {
        // (An empty answer is a row that is not listed any more, or a photo the catalogue does not know.)
        const answer = panel.row >= 0 ? panel.photoGrid.technicalInfoAt(panel.row) : ""
        panel.info = answer === "" ? {} : JSON.parse(answer)
    }

    function formatShutter(s) {
        if (s === null || s === undefined)
            return ""
        return s >= 1 ? qsTr("%1 s").arg(Math.round(s * 10) / 10)
                      : qsTr("1/%1 s").arg(Math.round(1 / s))
    }

    function formatAperture(a) {
        if (a === null || a === undefined)
            return ""
        const rounded = Math.round(a * 10) / 10
        return "f/" + (Number.isInteger(rounded) ? rounded.toFixed(0) : rounded.toFixed(1))
    }

    function formatIso(i) {
        return (i === null || i === undefined) ? "" : qsTr("ISO %1").arg(i)
    }

    function formatFocalLength(f) {
        return (f === null || f === undefined) ? "" : qsTr("%1 mm").arg(Math.round(f))
    }

    function formatOrientation(code) {
        switch (code) {
        case 1: return qsTr("Normal")
        case 2: return qsTr("Flipped horizontally")
        case 3: return qsTr("Rotated 180°")
        case 4: return qsTr("Flipped vertically")
        case 5: return qsTr("Rotated 90° counterclockwise, flipped")
        case 6: return qsTr("Rotated 90° clockwise")
        case 7: return qsTr("Rotated 90° clockwise, flipped")
        case 8: return qsTr("Rotated 90° counterclockwise")
        default: return ""
        }
    }

    function formatDate(t) {
        return (t === null || t === undefined) ? "" : new Date(t * 1000).toLocaleString(Qt.locale())
    }

    // The rows worth showing: label and value, in order, empty ones left out.
    function computeRows(i) {
        const out = []
        const add = (key, label, value) => { if (value !== "") out.push({ key, label, value }) }
        add("capture-time", qsTr("Capture date"), formatDate(i.capture_time))
        add("camera", qsTr("Camera"), i.camera || "")
        add("lens", qsTr("Lens"), i.lens || "")
        add("shutter", qsTr("Shutter speed"), formatShutter(i.shutter))
        add("aperture", qsTr("Aperture"), formatAperture(i.aperture))
        add("iso", qsTr("ISO"), formatIso(i.iso))
        add("focal-length", qsTr("Focal length"), formatFocalLength(i.focal_length))
        if (i.focal_length_35mm !== null && i.focal_length_35mm !== undefined && i.focal_length_35mm !== i.focal_length)
            add("focal-length-35mm", qsTr("35 mm equivalent"), formatFocalLength(i.focal_length_35mm))
        if (i.width && i.height)
            add("dimensions", qsTr("Dimensions"), i.width + " × " + i.height)
        add("orientation", qsTr("Orientation"), formatOrientation(i.orientation))
        add("gps-latitude", qsTr("Latitude"), i.gps_latitude || "")
        add("gps-longitude", qsTr("Longitude"), i.gps_longitude || "")
        // (The altitude is a distance; the reference says which side of sea level: "1" is below.)
        add("gps-altitude", qsTr("Altitude"),
            i.gps_altitude ? i.gps_altitude + (i.gps_altitude_ref === "1" ? " " + qsTr("below sea level") : "") : "")
        add("serial", qsTr("Serial number"), i.serial || "")
        return out
    }

    onRowChanged: refresh()
    Component.onCompleted: refresh()

    Connections {
        target: panel.photoGrid
        function onDataChanged() { panel.refresh() }
        function onModelReset() { panel.refresh() }
    }

    Label {
        anchors.centerIn: parent
        visible: panel.rows.length === 0
        text: qsTr("No photo to show")
        color: Theme.quiet
    }

    AppListFrame {
        anchors.fill: parent
        visible: panel.rows.length > 0

        AppListView {
            id: list
            anchors.fill: parent
            spacing: 6
            model: panel.rows

            delegate: ColumnLayout {
                id: delegateRoot
                required property var modelData
                readonly property string key: modelData.key
                readonly property string label: modelData.label
                readonly property string value: modelData.value
                width: list.rowWidth
                spacing: 2

                Label {
                    text: delegateRoot.label
                    color: Theme.quiet
                }
                Label {
                    Layout.fillWidth: true
                    text: delegateRoot.value
                    wrapMode: Text.Wrap
                }
            }
        }
    }
}
