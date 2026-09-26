# Browsing your photos

The **Cull** tab shows the catalogue as a grid of thumbnails, the newest photos first, with a filter bar above
it, a strip below it that describes what is selected, and the [keyword panel](06-keywords.md) on the right.

![The grid](images/grid.png)

## What a thumbnail shows

- Its **stars** at the top left, its **flag** at the top right (✔ picked, ✖ rejected; a rejected photo is also
  dimmed), a coloured bar along the bottom for its **colour label**.
- For photos that belong to a [series](05-series.md), a badge with the number of photos.

## Selecting

Selecting works as in a file manager.

| To select | Do |
| --- | --- |
| One photo | Click it |
| Add or remove one | `Ctrl`+click, or `Space` on the photo under the keyboard cursor |
| A range | `Shift`+click (or `Shift` with the arrow keys); `Ctrl+Shift`+click adds a range to what is selected |
| Several by dragging | Drag a rectangle over empty space (a rubber band); hold `Ctrl` to add |
| Everything | **Edit ▸ Select all** (`Ctrl+A`) |
| Nothing | `Escape`, or **Edit ▸ Select none** (`Ctrl+Shift+A`); clicking empty space does the same |
| The others | **Edit ▸ Invert selection** (`Ctrl+Shift+I`) |

The arrow keys, `PageUp`, `PageDown`, `Home` and `End` move the selection (the keyboard cursor); with `Ctrl` they
move the cursor and leave the selection alone. The strip under the grid describes the selected photo (its file,
camera, stars and flag), or says how many photos are selected.

Rating, flagging, labelling and keywords act on **everything selected** as one action (one `Ctrl+Z`), or on the
photo under the cursor when nothing is selected.

## Filtering

The filter bar narrows what the grid lists; the filters combine, and the count beside them says how many photos
they list.

- **All, 1+, 2+ … 5**: photos with at least that many stars.
- **Not rejected / All photos / Picked / Rejected**: rejected photos are hidden by default. Nothing is ever deleted;
  choose *Rejected* or *All photos* to see them.
- **The five coloured dots**: only photos with that colour label; click the same dot again to list all.
- **Series** (appears once the catalogue has series): all, in a series, unresolved series, resolved series; and
  **Open all / Close all** for every series.
- **Keyword**: choose *Show the photos with this keyword* in the keyword panel's menu; a chip *Keyword: Peru ×*
  appears, and its × removes the filter.

A photo you reject **stays where it is, dimmed**, until the list is next read (a filter change or **Refresh**), so
the grid does not shift under you while you cull. **Refresh** reads the list again: photos that arrived, rejected
photos that were left in place.

## The menu on a photo

A right click on a photo opens a menu: open it in the [image view](04-culling.md), give it a colour (including
purple, which has no key), pick, reject or clear its flag, and the series commands.

![The menu of a photo](images/grid-menu.png)

## Next

[Culling](04-culling.md).
