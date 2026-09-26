# FFmpeg

VidCrop releases include `vidcrop-ffmpeg` and `vidcrop-ffprobe`, unmodified builds of [FFmpeg](https://ffmpeg.org) 9.0, which does all of VidCrop's video work.

These builds are licensed under the GNU General Public License, version 3 or later (they include GPL parts such as x264). The full license text is at https://www.gnu.org/licenses/gpl-3.0.html.

Source code:

- FFmpeg: https://git.ffmpeg.org/ffmpeg.git (tag `n9.0`, or the exact revision printed by `vidcrop-ffmpeg -version`)
- macOS builds and their build scripts: https://ffmpeg.martin-riedl.de
- Windows and Linux builds and their build scripts: https://github.com/BtbN/FFmpeg-Builds

`tools/fetch_ffmpeg.py` in the VidCrop source records the exact download and checksum used for each release.
