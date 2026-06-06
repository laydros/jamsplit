# Creating marker files

This guide explains how to create the marker file that jamsplit uses to turn one
long recording into one MP3 per song.

Markers are song starts. Song 1 ends where song 2 starts, song 2 ends where song
3 starts, and so on. The last song runs to the end of the audio file. If the
first song starts at the beginning of the recording, add a marker at `0:00`.

## Audacity

Use an Audacity label export when you want a simple visual editor for finding
song boundaries.

1. Open Audacity and import the long recording.
2. Click at the start of the first song.
3. Choose `Edit > Labels > Add Label at Selection`, or use `Ctrl+B`.
4. Type the song title and press Enter.
5. Repeat for each song start.
6. Export the labels with `File > Export Other > Export Labels`.
7. Save the exported `.txt` file next to the recording, then use it as the
   jamsplit marker file.

Audacity exports labels as a tab-separated text file:

```text
0.000000	0.000000	Opening Jam
323.500000	323.500000	Slow Blues
3731.000000	3731.000000	Closer
```

jamsplit uses only the first number on each line, which is the label start time.
The second number is Audacity's label end time and is ignored. Point labels and
range labels both work, but jamsplit treats both as song-start markers.

Notes:

- Keep the label text short enough to make a useful filename.
- Blank labels are allowed; jamsplit will name them `Untitled Song N`.
- If you have multiple label tracks, export the one that contains the song
  starts you want to use.
- If jamsplit detects the file as `audacity`, no extra format setting is needed.

## REAPER

Use a REAPER marker or region export when the session is already laid out in
REAPER.

1. Open the project containing the long recording.
2. Set the project/ruler time display to minutes and seconds, not bars and
   beats.
3. Add a marker at each song start, or create one region per song.
4. Name each marker or region with the song title.
5. Open `View > Region/Marker Manager`.
6. In the manager, include the markers and/or regions you want to export.
7. Use the manager's export command to export regions/markers to a `.csv` file.
8. Use that `.csv` file as the jamsplit marker file.

The exported file should look roughly like this:

```csv
#,Name,Start,End,Length
M1,Opening Jam,0:00.000,,
R1,Slow Blues,5:23.500,9:00.000,3:36.500
M2,Closer,1:02:11.000,,
```

jamsplit reads rows whose `#` value starts with `M` or `R`. It uses the `Name`
column as the title and the `Start` column as the song start. Region `End` and
`Length` values are ignored.

If validation says the start time looks like bars/beats, re-export after
changing REAPER's time display to minutes and seconds. A bars/beats value like
`9.1.00` is not accepted because it is not an audio timestamp.

## Plain text

The plain text format is the fallback/manual option. Use it when you do not want
to open Audacity or REAPER, or when you already wrote down the song start times
by hand.

Create a `.txt` file with one song start per line:

```text
# comments and blank lines are ignored
0:00 Opening Jam
05:23 - Slow Blues
1:02:11    Closer
3722.5 Encore Noodle
```

Accepted timestamp forms:

- `H:MM:SS`, such as `1:02:11`
- `M:SS`, such as `05:23`
- raw seconds, such as `3722.5`

Fractions are allowed in the final component, so `5:23.5` and `3722.5` are
valid. The title is everything after the first whitespace or dash separator. A
line with only a timestamp is valid; jamsplit will generate an `Untitled Song N`
title.

The CLI auto-detects this format when the file does not look like an Audacity
label export or a REAPER CSV:

```bash
jamsplit validate --audio jam.wav --markers songs.txt
```

If auto-detection guesses wrong, force plain text:

```bash
jamsplit validate --audio jam.wav --markers songs.txt --format plain
jamsplit split --audio jam.wav --markers songs.txt --format plain
```

In the GUI, choose `plain` from the `Format` dropdown.
