# Series

A **series** is a group of photos that belong together: the frames of a burst, or the exposures of a bracket. A
series is shown in the grid as **one thumbnail** with the number of photos, so a burst of twenty frames takes one
place instead of twenty.

![Two series, closed](images/series-collapsed.png)

## How series form

When photos arrive (at the end of a scan or an import), Auroraw groups the photos that

- come from **the same camera**,
- were taken **at most 2 seconds apart** (this gap is a setting), and
- have a capture time (a photo with no date never joins a series).

A series has at least two photos, and at most 100 (a long run of shots is cut). It is a **bracket** when it has three
or more photos with the same aperture, ISO and focal length and different exposure times, and a **burst**
otherwise. Its **cover**, the picture shown when it is closed, is its first frame.

In **File ▸ Settings…** you can change the gap and press **Regroup the series now**: the series that were made
automatically and are not resolved are formed again with the new gap. Series you made by hand, and series you
resolved, are never touched.

## Opening and closing

Click a series' badge (▣ and the count), or press `E` with the cursor on it, to **open** it: its photos appear in
place, **in the order they were taken**, joined by a line under them. The badge (▾) or `E` closes it. **Open all**
and **Close all** in the filter bar do every series at once. Opening the image view on a closed series opens it and
starts at its first frame.

![A series, open](images/series-open.png)

## A closed series is one thing

Selecting a closed series selects **all** its photos, so what you do to it happens to all of them, in one step:
`X` rejects the whole burst, `5` gives every frame five stars, a keyword is given to all of them. Open the series
to treat its photos one by one.

## Resolving a series

Resolving is choosing the frames you keep from a burst. Open the series, select the frame or frames to keep (click,
`Ctrl`+click for several) and press `R` (or *Resolve the series* in the photo's menu). The **kept photos are
picked**, and **all the others of the series are rejected**, in one step. The series is marked resolved (a ✓ on its
badge) and stays a series; the rejected frames leave the default list, and *All photos* shows them again.
`Ctrl+Z` puts every flag back as it was. *Reopen the series* (in the menu) marks it unresolved again and leaves
the flags alone. `R` on a closed series only opens it, for you to choose.

## Making and unmaking series by hand

- **Group**: select photos and press `Ctrl+G` to make them a new series. A photo leaves the series it was in; a
  series left with a single photo disappears. To **merge** two series, select both (they are selected whole when
  closed) and group; to **split** one, open it, select some of its photos and group.
- **Take out**: `Ctrl+Shift+G` takes the selected photos out of their series, or dissolves a closed series that is
  selected.
- Both are steps you can undo.

## Filtering

The **Series** menu in the filter bar lists all photos, only photos in a series, only those of **unresolved**
series (what is left to sort out), or only those of **resolved** ones.

## Next

[Keywords](06-keywords.md).
