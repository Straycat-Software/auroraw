// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import org.auroraw.ui

// The frame of every banner under the header (D-135): a full-width bar in one of exactly two tones, named by
// what the banner says and not by a colour, so that a third cannot appear without a decision. `notice` is for
// something that could not be done (`NoticeBar`); `info` is for news that offers an action (`CardBanner`, a
// camera card was inserted; `ExternalBanner`, another application changed metadata). Both have a 1px edge.
// The banner lays out its own content and sets its own height.
Rectangle {
    id: banner
    property string tone: "info"
    readonly property bool isNotice: tone === "notice"

    color: isNotice ? Theme.noticeGround : Theme.infoGround
    border.color: isNotice ? Theme.noticeEdge : Theme.infoEdge
    border.width: 1

    Component.onCompleted: {
        if (tone !== "notice" && tone !== "info")
            console.warn("AppBanner: unknown tone \"" + tone + "\", expected \"notice\" or \"info\"")
    }
}
