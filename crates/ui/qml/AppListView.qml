// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtQuick.Controls
import org.auroraw.ui

// A scrolling list of rows, the way every list of this interface is one (D-132): it does not
// rubber-band past its ends (D-094 already ruled that out), and it carries an `AppScrollBar`.
//
// An overlay scrollbar paints its thumb on the last pixels of its own view, over whatever a row draws
// there, and shrinking the view itself moves the thumb along with it — so the two only stop touching
// if each *row* stops short of the view's edge. `rowWidth` is that width: a delegate sizes itself to
// `<thisList's id>.rowWidth`, never to the view's own `width`, and cannot forget the scrollbar half of
// the pairing, since the scrollbar is this component's own.
ListView {
    id: view
    readonly property int rowWidth: width - scrollbar.width
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: AppScrollBar { id: scrollbar }
}
