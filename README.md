# VidCrop

**Crop your video. That's it. That's the app.**

Drag the edges to make the picture smaller. Select the boring parts of the timeline and hit Delete. Press Space to watch it. Press Save. Go outside. Touch grass.

## Why this exists

I wanted to cut the edges off a video and chop out the part where I fumbled with the mouse. A thing a toaster should be able to do. Here is how that went:

- **Avidemux:** Opened it. Saw a window that looks like it was designed in 2003 by someone who hates me personally. Found the crop filter three menus deep, where you type pixel counts into little boxes like you're filing taxes. Then I got to pick a video encoder, a container, and an audio codec. I don't want to pick a codec. I want the video to be smaller.
- **The Photos app:** Can crop, can trim. Can trim *one* piece off the start and *one* piece off the end. Want to cut something out of the middle? Ha. No. Go away. And getting even that far means first importing the video into a Library, then hunting for which button is Edit, which tab is crop, and which tiny yellow handle is trim. I'm sure it all makes perfect sense once I've been fully assimilated into the Apple collective. Resistance is futile. I resisted anyway.
- **LosslessCut:** Genuinely impressive, genuinely lossless, and genuinely requires me to understand keyframes, segments, tracks, and an export dialog with more checkboxes than a pilot's pre-flight list. Also it doesn't really do the crop part, which was, you know, the thing.
- **CapCut:** Wants me to sign in before it lets me get anything done. Then it's a wall of templates, stickers, AI effects, auto-captions, trending sounds, and little "Pro" badges on things I didn't ask for. Somewhere under all the confetti there's a crop tool. I wanted to trim a screen recording, not launch a TikTok career.
- **OBS Studio (for recording the screen in the first place):** Scenes. Sources. Canvas resolution versus output resolution. Encoder presets, rate control, keyframe intervals, audio mixer, "Studio Mode." Forget the college class, this one wants a PhD. I wanted to press Record and then press Stop. It wanted me to become a TV broadcast engineer.
- **Assorted "free online video croppers":** Upload your 2 GB file to a stranger's server, wait, get a watermark, get asked for a credit card. Pass.
- **ffmpeg by hand:** `ffmpeg -i in.mp4 -filter_complex "[0:v]trim=0:12.4,setpts=PTS-STARTPTS,crop=1280:720:320:180[v0];..."` Yes, I'm sure. Totally normal thing to type to delete a clip.

Let that sink in. **It was less work to write my own video editor from scratch than to figure out how to operate any of these.** Not "a little quicker." Less work. Writing. An entire app. Versus pressing buttons in someone else's.

That's not a brag about me. It's an indictment of them. Somewhere along the way, "crop a video" turned into a skill you have to go to school for, and every app decided you secretly wanted 40 features you'll never touch, stacked on top of the one you came for.

So here's the app I actually wanted. If you can drag a rectangle and press Delete, you already know how to use it. There is no manual. There doesn't need to be.

## What it does

- **Crop the picture:** drag any edge or corner of the box over the video. What's inside the box is what you keep.
- **Cut the timeline:** drag across the timeline to select a stretch, press Delete. Do it as many times as you want, anywhere in the video. Beginning, middle, end, who cares.
- **Preview:** Space plays it back *with the cuts already skipped*, so you see what you'll get.
- **Undo:** Cmd+Z / Ctrl+Z. Because you will.
- **Save:** one button. It writes a new file next to the original. Your original is never touched.
- **Record your screen:** hit Record, do your thing, hit Stop, and it drops straight into the editor so you can crop out your messy desktop and cut the part where you looked for the right window.

## What it does *not* do

Titles. Transitions. Color grading. Stickers. An "AI Magic Enhance" button. A timeline with 14 tracks. A subscription. If you need those, there are dozens of apps that will happily bury you in them.

## Platforms

macOS first (it's what I'm sitting at), then Windows and Linux. Built with [Tauri](https://tauri.app) and [ffmpeg](https://ffmpeg.org), so it's small, fast, and runs entirely on your own computer. Nothing is uploaded anywhere, ever.

## Download

**[62-84-178-253.sslip.io/VidCrop](https://62-84-178-253.sslip.io/VidCrop/)**: Mac, Windows and Linux. Your computer will call it unidentified and suspicious. It is neither. The download page shows how to open it anyway.

## Status

Works on macOS: crop, cut, preview, save, fast save, screen recording. Windows and Linux are next. The plan lives in [specs/](specs/README.md).

## Robots welcome

Everything the buttons do, a script can do too, through a local API or the `vidcrop` command:

```sh
vidcrop export talk.mp4 --crop 1280:720:320:180 --cut 0-4.5 --cut 61-75
```

See [specs/api.md](specs/api.md).

## Building it yourself

You need Rust, Node and ffmpeg (`brew install rust node ffmpeg` on a Mac).

```sh
npm install
npx tauri dev          # run the app while working on it
npx tauri build        # make VidCrop.app
cargo test             # tests, including a real crop-and-save
```
