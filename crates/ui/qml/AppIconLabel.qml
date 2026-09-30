// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A line of text whose `sentence` may hold the characters the interface used before it had icons (the star, the tick,
// the cross...), shown as the icons that replace them (D-137): for a sentence that comes from elsewhere, such as the
// summary the engine writes under the grid. It is made of runs, the icons in the icon font, chosen by its family, and
// the rest in the text's own, because no other way chooses the font on every platform and Qt (`Text.StyledText` ignores
// the `face` of a `<font>` tag, `Text.RichText` does not elide, and a font's list of fallbacks is not in Qt 6.4: left to
// the system, a private-use character is drawn by whichever font maps it). When the line is too narrow, the last run of
// text is elided.
//
// The runs sit in a `Row`, and the widths are measured here (`FontMetrics`), not left to a layout: a `RowLayout` whose
// `Repeater` changes its runs with the sentence crashes Qt 6.4.2 in its rearrangement of the deleted items.
Control {
    id: control

    property string sentence: ""
    property color color: Theme.surface.text
    property alias runItems: runs

    FontMetrics { id: textFont; font: control.font }
    FontMetrics { id: iconFont; font.family: Icons.family; font.pixelSize: control.font.pixelSize }

    readonly property var runList: Icons.runs(sentence)
    // The width of each run, as it is set (an icon is one em wide), and of the line if nothing were elided.
    readonly property var natural: {
        void textFont.font
        void iconFont.font
        return runList.map(run => run.icons ? iconFont.advanceWidth(run.text) : textFont.advanceWidth(run.text))
    }
    readonly property real naturalWidth: natural.reduce((sum, width) => sum + width, 0)
    // The run that gives room: the last of text (an icon does not shrink).
    readonly property int elidable: {
        for (let i = runList.length - 1; i >= 0; i--)
            if (!runList[i].icons)
                return i
        return -1
    }
    // What is shown: each run with its width, the elidable one cut to the room there is.
    readonly property var laid: {
        void textFont.font
        const over = naturalWidth - Math.max(0, availableWidth)
        return runList.map((run, i) => {
            if (i !== elidable || over <= 0)
                return { text: run.text, icons: run.icons, width: natural[i] }
            const text = textFont.elidedText(run.text, Qt.ElideRight, Math.max(0, natural[i] - over))
            return { text: text, icons: false, width: textFont.advanceWidth(text) }
        })
    }

    implicitWidth: naturalWidth + leftPadding + rightPadding
    implicitHeight: textFont.height + topPadding + bottomPadding
    Accessible.role: Accessible.StaticText
    Accessible.name: sentence

    contentItem: Row {
        Repeater {
            id: runs
            model: control.laid
            Loader {
                id: run
                required property var modelData
                width: modelData.width
                height: textFont.height
                sourceComponent: modelData.icons ? iconRun : textRun

                Component {
                    id: textRun
                    Label {
                        width: run.width
                        height: run.height
                        verticalAlignment: Text.AlignVCenter
                        text: run.modelData.text
                        textFormat: Text.PlainText   // a '<' is what it is
                        color: control.color
                        Accessible.ignored: true
                    }
                }
                Component {
                    id: iconRun
                    Label {
                        width: run.width
                        height: run.height
                        verticalAlignment: Text.AlignVCenter
                        text: run.modelData.text
                        color: control.color
                        font.family: Icons.family
                        font.pixelSize: control.font.pixelSize
                        font.hintingPreference: Font.PreferNoHinting
                        Accessible.ignored: true
                    }
                }
            }
        }
    }
}
