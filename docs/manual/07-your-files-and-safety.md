# Your files and their safety

## Your photos are never changed

Auroraw **never modifies, moves, renames or deletes your original photo files**, and it never deletes anything
you have done: removing a source or dissolving a series moves what it kept to a recoverable place. The **Delete**
in the Edit menu acts on text you are typing, never on photos. Rejecting a photo only gives it a flag.

The exceptions are what *you* ask for: import copies photos to the destination you choose, and verifies each copy; and
**Tools ▸ Export XMP files…** writes small XMP files *beside* your originals (never the originals themselves), as
described [below](#writing-xmp-files-for-other-applications).

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

Other software (Lightroom, darktable, digiKam, ExifTool...) may keep an **XMP file** next to a photo. Auroraw reads
it when it scans, and writes to it only when you export XMP files (next section). When a rescan finds that such a file changed since Auroraw last read it, a banner
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

## Writing XMP files for other applications

**Tools ▸ Export XMP files…** (`Ctrl+Shift+E`) writes, next to the original of each photo you ask for, the XMP file
that other software reads: the rating (a rejected photo as −1 if you like), colour label, title, caption,
keywords, the copyright and other information fields, and the capture data. Nothing is written until you press
**Export**, and nothing is exported on its own: it is one way, from Auroraw to the file.

- **Which photos.** The selected photos, or every photo of one source.
- **Name of a new file.** `photo.xmp`, or `photo.ARW.xmp` (the whole file name). When two photos of a folder share
  a name (`A.png` and `A.tif`), both get the whole-file form, since `photo.xmp` would belong to neither.
- **When a file already exists** (Lightroom's, darktable's, digiKam's, or an earlier export of yours), the file that
  belongs to the photo is the one written, whatever the naming:
  - **Merge into it** (the default): only what Auroraw owns (the fields above) is rewritten. The other
    application's develop settings and everything else in the file stay.
  - **Replace it**: after a confirmation, the old file is first kept in the workspace's `removed/` folder (nothing
    is deleted), and a new one is written.
  - **Leave it alone**: only the missing files are written.
- **Keywords marked "Do not export"** (and the keywords below one) stay out of the files.

**A file another application changed since Auroraw last looked is not overwritten.** It is *held back*: the
banner *N photos have metadata changed by another application* appears, and the result offers **Review changes…**
(or **Review…** on the banner), which lets you take the change or decline it, as in the previous section. Then export
again. A file that is not XMP is never touched, unless you chose
*Replace it*. A source that cannot be reached (a card that is not there) is reported, and the rest goes on.

Exporting again when nothing changed writes nothing. Auroraw does not take its own export for another
application's change. **An export changes no photo**, so there is nothing to undo; a cancelled export keeps what it
already wrote.

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
