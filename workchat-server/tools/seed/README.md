# Seed tool

Fills a WorkChat server (1.1.0 or newer) with a believable company so the apps can be tried and screenshotted. Everything goes through the HTTP API
(admin API for users, bots and channels; each person's own login for what they write); the database is never touched.

```sh
./seed.sh --server http://192.168.8.101:18600 --owner-password '<owner password>' --password '<password for all 10 users>'
```

Options: `--owner admin` (default), `--days 14` (messages are spread over about this many recent days), `--assets DIR` (default `generated/`), `--regenerate`
(draw the sample files again), `--random-seed`, `--pause` (seconds between requests; the server allows about 40 requests per second per address).
The first run creates a Python virtual environment in `.venv/` (git-ignored; Pillow, reportlab, python-docx, openpyxl, python-pptx) and needs `ffmpeg` on the PATH.
It takes about a minute. If any of the sample users already exists it changes nothing and exits with code 3, so seed a fresh server (or delete the users in the admin panel).

## What it creates

* **10 users** with a job title, phone number and (8 of them) a drawn avatar; `liam.chen` and `grace.nguyen` have none, to show initials:
  katherine.wells (Head of Product Operations, the main persona), david.kim, marcus.brody, priya.patel, liam.chen, elena.vance, sarah.mitchell, alex.rivera,
  omar.haddad, grace.nguyen. All have the password you pass.
* **10 channels** with topic, picture and members: company-announcements (everyone), engineering-deployments, product-strategy, design-system, executive-leadership,
  sales-pipeline, people-culture, legal-compliance, incident-response, random-lunch (everyone); a **group chat** `launch-war-room`; **8 direct messages**.
* **2 bots** (`deploybot` in engineering-deployments and incident-response, `remindbot` in company-announcements and executive-leadership) created through the admin API
  with a picture; they post as normal members using their bot token.
* **30 top-level messages in every channel, DM and the group chat** (570), plus 75 thread replies in 30-odd threads, with 175 reactions, edits, 22 pinned messages
  (pinned by the owner), 2 deleted messages, quote replies, @mentions, and rich text in the server's markup: `*bold*`, `_italic_`, `~strike~`, `` `code` ``, fenced
  code blocks, `> quote`, `- bullets`, `1. numbered`.
* **Files**: 38 photos/screenshots (JPG and PNG, 640 to 2400 px wide), 8 videos (4 distinct H.264 + AAC clips, 120 KB to 580 KB, landscape, portrait and square, with poster
  frame and size details), 19 voice notes (8 synthetic recordings, MP4 AAC and Ogg Opus, with duration and a 64-value waveform, named `voice-<date>-<time>.m4a|ogg`),
  12 DOCX, 11 XLSX (with formulas and a chart), 8 PPTX, 9 PDF (an invoice, a 5-page postmortem, a 28-page audit; `pages` detail set), a CSV, 3 TXT and a ZIP.
  All files are invented and drawn or generated locally; nothing is downloaded.
* **Timestamps** are backdated over the last two weeks through `PUT /admin/messages/{id}/ts`, in Perth daytime hours, so day separators and "Yesterday" appear.
  Chats end at different times (some minutes ago, some days ago).
* katherine.wells has three favourite chats (executive-leadership, product-strategy, the DM with Elena), a muted channel (random-lunch), unread messages in the chats where others had the
  last word, and threads with unread replies. The other users have random unread tails.

## Files of the tool

`seed.py` (the runner), `people.py` (users, channels, bots), `script_channels.py` and `script_dms.py` (the conversations; the format is described at the top of the first),
`assets.py` with `gen_images.py`, `gen_media.py`, `gen_docs.py` (sample files into `generated/`, with `manifest.json`).
To add a conversation, add an entry to the scripts (30 messages; the runner warns otherwise) and, for a new channel, to `people.CHANNELS`.
