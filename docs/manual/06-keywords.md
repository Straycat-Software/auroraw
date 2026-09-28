# Keywords

Keywords describe what is in a photo (*Animals*, *Peru*, *Cusco*). Auroraw keeps one **vocabulary** of keywords,
arranged as a tree (*Places ▸ Peru ▸ Cusco*), and you give photos the keywords they deserve. The panel on the
right of the grid has four tabs, **Keywords**, **Metadata**, **Info** and **Collections**; `Ctrl+K`
(**Tools ▸ Keywords**) switches to the first and puts the keyboard in its field, `Escape` gives it back to the
grid, and the `«` and `»` buttons fold the panel away or bring it back. Drag its left edge to make it wider or
narrower (a double click gives back the default width); it remembers its width, whichever tab is showing.

## Giving keywords to photos

Select photos in the grid. In the panel, each keyword has a check that shows **none, some or all** of the selected
photos carry it, and a number that says how many photos have it in the whole catalogue. Click the check to give
the keyword to **all** the selected photos, or to take it off all of them: one step, undone with `Ctrl+Z`.

![The keyword panel](images/keywords-add.png)

## Finding and adding a keyword

The field at the top is for **type-ahead**: it filters the tree as you type (a match shows with the keywords above
it). Then:

- `Enter` gives the best match to the selected photos.
- If nothing matches, `Enter` **creates** the keyword and gives it to the selected photos, as one step.
- To add a keyword whose name exists **elsewhere in the tree** (a *Fox* under *Animals* and another at the top
  level), click the button that appears under the field (*Add "Fox" at the top level*), or press `Shift+Enter`.
- Two keywords under the same parent cannot have the same name (whatever the case).

Where a new keyword goes: at the top level, unless you clicked a keyword first: the line under the field then says
*New keywords go under Animals*, and the × puts that back.

## Organising the vocabulary

Right-click a keyword (or use its menu) for:

- **Show the photos with this keyword**: filters the grid (and its sub-keywords).
- **Rename…**: every photo that has it follows.
- **Move to…** and **Move to the top level**, or **drag and drop** a keyword onto another one to make it a child of
  it (drop it anywhere in the panel with no keyword under it to make it a top-level keyword). A keyword cannot be
  moved under itself.
- **Delete…**: deletes the keyword **and the keywords under it**; the confirmation says how many keywords and photos
  are affected. The photos lose those keywords.
- **Properties…**: sets **synonyms** (one a line — typing a synonym in the field above finds the keyword it belongs
  to, the same as its own name) and **Do not export**, which will keep the keyword out of exported photos once
  exporting itself exists.

![Dragging a keyword](images/keywords-drag.png)

Creating, renaming, moving, deleting keywords and setting their properties can all be undone with `Ctrl+Z`, and
deleting brings back the same keywords on the same photos.

## Where keywords are kept

A photo's keywords are written in its sidecar, with the vocabulary in the workspace, so they survive rebuilding
the catalogue and can be backed up with the workspace (see [Your files and their safety](07-your-files-and-safety.md)).

## Metadata

The **Metadata** tab edits a photo's title, caption, creator, copyright and the rest of its plain-text IPTC and
XMP fields — one field a line, except **Persons shown**, which takes one name a line. With
several photos selected, a field where they disagree shows *Multiple values*; typing in it and leaving the
field (`Tab`, a click elsewhere, or `Enter`) sets it on **all** the selected photos, one step, undone with
`Ctrl+Z`. Leaving a field empty clears it.

![The metadata panel](images/metadata.png)

## Info

The **Info** tab shows the active photo's own technical metadata — camera, lens, exposure (shutter speed,
aperture, ISO, focal length), dimensions, orientation, GPS position and serial number, when the file carries
them — read-only, and always for the one photo the grid's cursor is on, not a selection. A field the photo
does not carry (most photos have no GPS position, for instance) is left off the list rather than shown empty.

![The Info panel](images/info.png)

## Collections

The **Collections** tab groups photos by hand, for anything the vocabulary is not about — an album, a delivery,
a shortlist. A collection can hold photos and also hold other collections (a folder inside a folder), shown as
a tree the same way the keyword vocabulary is.

Select photos and type a name in the field at the top: `Enter` puts the selection in the best-matching
collection, or **creates** one with that name and puts the selection in it, as one step, the same as the
keyword field. The check beside each collection shows **none, some or all** of the selected photos are in it;
click it to put the whole selection in, or take it out. Right-click a collection (or use its menu) for
**Rename…**, **Move to…** (or drag and drop it onto another collection, or onto empty space in the tab for the
top level) and **Delete…**, which takes the collection and what is inside it away — the photos themselves are
never touched, only their membership. **Show the photos in this collection** filters the grid by it and by
whatever is inside it. Every one of these is undone with `Ctrl+Z`.

![The collections tab](images/collections.png)

Smart collections (filled automatically by a saved search) come later.

## Next

[Your files and their safety](07-your-files-and-safety.md).
