// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// One icon of the interface (D-137), by `name` (see `Icons.qml`, and the icon sheet of the design system): a square
// `size` pixels on a side, in `color`. It is text set in the icon font, so it is sharp at any size and is coloured like
// text; the meaning colours (`rating`, `picked`, `danger`) are set by the caller, as they were for the glyph. An icon
// says nothing to a screen reader: the control or chip that holds it is the one that has a name.
Text {
    id: icon
    property string name: ""
    property int size: 15

    text: Icons.glyph(name)
    color: Theme.surface.text
    width: size
    height: size
    horizontalAlignment: Text.AlignHCenter
    verticalAlignment: Text.AlignVCenter
    font.family: Icons.family
    font.pixelSize: size
    font.weight: Font.Normal
    font.hintingPreference: Font.PreferNoHinting
    Accessible.ignored: true
}
