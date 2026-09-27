// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// A histogram (D-103): the brightness (grey, filled) and the three colours (thin lines) of a picture, from the
// numbers the image view measured on it (`data`: four arrays of 256 counts, luma, red, green and blue). It says
// what the camera's preview holds, not the RAW data.
Rectangle {
    id: histogram
    property var data: null
    // The share of pixels in the highlights' clip and in the shadows' (0 to 1), written under the drawing.
    property real high: 0
    property real low: 0
    property alias canvas: canvas

    width: 240
    height: 110
    radius: 4
    color: "#c0000000"

    onDataChanged: canvas.requestPaint()

    Canvas {
        id: canvas
        anchors.fill: parent
        anchors.margins: 6
        anchors.bottomMargin: 20
        onPaint: {
            const ctx = getContext("2d")
            ctx.clearRect(0, 0, width, height)
            if (!histogram.data)
                return
            // The tallest bar, ignoring the two ends (a clipped picture piles up there and would flatten the rest).
            let top = 1
            for (const channel of histogram.data)
                for (let i = 2; i < 254; i++)
                    top = Math.max(top, channel[i])
            const x = i => i / 255 * width
            const y = v => height - Math.min(1, v / top) * height
            function line(channel, style, fill) {
                ctx.beginPath()
                ctx.moveTo(0, height)
                for (let i = 0; i < 256; i++)
                    ctx.lineTo(x(i), y(channel[i]))
                ctx.lineTo(width, height)
                if (fill) {
                    ctx.fillStyle = style
                    ctx.fill()
                } else {
                    ctx.strokeStyle = style
                    ctx.lineWidth = 1
                    ctx.stroke()
                }
            }
            line(histogram.data[0], "rgba(200,200,200,0.45)", true)
            line(histogram.data[1], "rgba(255,80,80,0.9)", false)
            line(histogram.data[2], "rgba(80,220,80,0.9)", false)
            line(histogram.data[3], "rgba(90,140,255,0.9)", false)
        }
    }
    Text {
        anchors.fill: parent
        anchors.margins: 4
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignBottom
        font.pixelSize: 11
        color: "#dddddd"
        text: qsTr("Shadows %1 %  ·  Highlights %2 %")
                  .arg(Math.round(histogram.low * 1000) / 10).arg(Math.round(histogram.high * 1000) / 10)
    }
}
