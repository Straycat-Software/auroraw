# Your files and their safety

## Your photos are never changed

Auroraw **never modifies, moves, renames or deletes your original photo files**, and it never deletes anything
you have done: removing a source or dissolving a series moves what it kept to a recoverable place. The **Delete**
in the Edit menu acts on text you are typing, never on photos. Rejecting a photo only gives it a flag.

The one exception is what *you* ask for: import copies photos to the destination you choose, and verifies each copy.

## What is written where

- **Your workspace** (the folder you chose, by default in `Pictures/Auroraw`) holds what you decided:
  - `photos/` has one small file per photo (an **XMP sidecar**) with its rating, flag, colour label, keywords and the
    information read from the original;
  - `state/` holds the keyword vocabulary and the list of sources, and `series/` the series;
  - `removed/` holds what was taken out when you removed a source, so it can be restored;
  - `workspace.json` and `README.txt` say what the folder is.
  Metadata is written **as soon as you make a change**, and a write that is interrupted cannot damage a file.
- **The catalogue** (the database the grid reads) and the **thumbnails** live in the application's data and cache
  folders on this computer, not in the workspace. They are an *index*: if they are lost, Auroraw **rebuilds the
  catalogue from the workspace** (a few seconds for tens of thousands of photos), and makes the thumbnails again.

## Backing up

Back up the **workspace folder** (copy it, or use your usual backup or sync tool) and your **original photos**.
That is everything: on a new computer, open the copied workspace and re-add the sources, and the catalogue is
rebuilt. Two computers can share a workspace folder through an ordinary sync tool, as long as only one runs
Auroraw on it at a time.

## When another application changes a photo's XMP file

Other software (Lightroom, darktable, digiKam, ExifTool...) may keep an **XMP file** next to a photo. Auroraw only
ever reads it, never writes it. When a rescan finds that such a file changed since Auroraw last read it, a banner
says *N photos have metadata changed by another application*, with **Review…** and **Ignore**.

**Review…** lists the photos with what changed, *yours → the file's*: the rating, colour label, title, caption,
the copyright and other information fields, and the keywords the file gained or lost. **Accept** (or **Accept
all**, one click) applies the changes; **Ignore** leaves your photos as they are. Either way, Auroraw remembers
the file as it now stands and does not mention it again until it changes again. Accepting is one step of the undo
(*Undo accepting external changes*), and a keyword the file names that you do not have yet is added to your
keyword list.

Only what the file changed is offered, so your own work is not touched: a field you changed and the file did not
stays as you have it. When **both** changed the same field to different values, the review shows both and keeps
**yours** unless you choose *Take the file's*.

## What can be undone

Ratings, flags, colour labels, keywords and the vocabulary, series (grouping, taking out, resolving), accepting
another application's changes, each as one step, with `Ctrl+Z`. Undo is not kept after you close the workspace.

Adding or removing a source and importing are not undone with `Ctrl+Z`; removing a source can be reversed by
adding the folder again and choosing *Restore them*.

## If something looks wrong

- **A photo is missing from the grid**: check the filters (rejected photos are hidden by default, and the rating,
  colour, series and keyword filters combine).
- **Thumbnails do not appear**: a photo whose file is unreadable, or a RAW file with no preview, says *No preview*.
- **A source is Offline**: reconnect it and choose **Rescan**.
