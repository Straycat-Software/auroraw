# Keywords

Keywords describe what is in a photo (*Animals*, *Peru*, *Cusco*). Auroraw keeps one **vocabulary** of keywords,
arranged as a tree (*Places ▸ Peru ▸ Cusco*), and you give photos the keywords they deserve. The panel on the
right of the grid has two tabs, **Keywords** and **Metadata**; `Ctrl+K` (**Tools ▸ Keywords**) switches to the
first and puts the keyboard in its field, `Escape` gives it back to the grid, and the `«` and `»` buttons fold
the panel away or bring it back. Drag its left edge to make it wider or narrower (a double click gives back the
default width); it remembers its width, whichever tab is showing.

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

![Dragging a keyword](images/keywords-drag.png)

Creating, renaming, moving and deleting keywords can all be undone with `Ctrl+Z`, and deleting brings back the
same keywords on the same photos.

## Where keywords are kept

A photo's keywords are written in its sidecar, with the vocabulary in the workspace, so they survive rebuilding
the catalogue and can be backed up with the workspace (see [Your files and their safety](07-your-files-and-safety.md)).

## Metadata

The **Metadata** tab edits a photo's title, caption, creator, copyright and the rest of its plain-text IPTC and
XMP fields — one field a line, except **Creator** and **Persons shown**, which take one name a line. With
several photos selected, a field where they disagree shows *Multiple values*; typing in it and leaving the
field (`Tab`, a click elsewhere, or `Enter`) sets it on **all** the selected photos, one step, undone with
`Ctrl+Z`. Leaving a field empty clears it.

![The metadata panel](images/metadata.png)

## Next

[Your files and their safety](07-your-files-and-safety.md).
