// SPDX-License-Identifier: GPL-3.0-or-later
pragma Singleton
import QtQuick

// The look of the application (D-094, refreshed by D-127): a cool-neutral environment on Fusion,
// no colour cast around a photograph, colour only for what is selected, focused or warned about.
// D-127 (the visual refresh) replaced the single flat grey with a four-step surface ladder — an
// elevated `raised` tier, lighter than the canvas, for dialogs and popups, is new; Fusion's old
// convention painted a popup darker than the window behind it — and gave buttons, fields and the
// spin box a flat, custom-drawn look instead of Fusion's default bevel gradient.
QtObject {
    readonly property var surface: ({
        sunken: "#1c1d21",
        window: "#2d2e34",
        raised: "#3f424a",
        hover: "#4d515b",
        border: "#606471",
        text: "#eaeaeb",
        placeholder: "#787b87"
    })

    // The colours that mean something.
    readonly property color accent: "#5e99d4"
    // A second, sparing accent (D-127): the cover, the About link, a rare highlight — never a
    // second meaning for a status, which stays rating/picked/danger/warning/label.
    readonly property color accentSecondary: "#3ea89d"
    readonly property color rating: "#dbb04d"
    readonly property color warning: "#dea645"
    readonly property color picked: "#57b77a"
    readonly property color danger: "#df6f62"
    readonly property color quiet: "#9fa1a8"

    // A colour label (spec §5.3), by the name the grid gives it.
    function labelColour(name) {
        switch (name) {
        case "red": return "#df6158"
        case "yellow": return "#e2bb46"
        case "green": return "#54bb76"
        case "blue": return "#5da0da"
        case "purple": return "#a37fcc"
        }
        return "transparent"
    }

    // Two radius tiers (D-127): controls (buttons, fields, chips) are the smaller one; dialogs,
    // popups and floating chrome are the larger one, so a container reads as distinct from the
    // flatter controls inside it.
    readonly property int radiusControl: 4
    readonly property int radiusContainer: 10

    // The one motion exception (D-127) in an interface that otherwise has none: a colour/opacity
    // fade on hover, press, focus and selection, nowhere else. `Behavior on color` and friends use
    // this duration; nothing is animated in position or size.
    readonly property int motion: 150

    // The typeface: IBM Plex Sans, carried by the application (`assets/fonts`), at 11 points.
    // `AURORAW_FONT` (a family) and `AURORAW_FONT_SIZE` (points) try others without a rebuild.
    readonly property string defaultFamily: "IBM Plex Sans"
    readonly property int defaultSize: 11
    property string fontFamily: ""
    property int fontSize: 0
}
