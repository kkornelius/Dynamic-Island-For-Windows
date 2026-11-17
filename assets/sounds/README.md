# Water check-in sound

`water-gulp.wav` is embedded in the executable and plays on an explicit **I had some water** check-in. It is a short sequence of three recorded human swallows, not synthesized speech.

Source: [Gulp - Hard Swallow](https://freesound.org/people/magnuswaker/sounds/531755/) by **magnuswaker**, published August 21, 2020 on Freesound. The source page marks the recording **CC0 1.0 Universal**: https://creativecommons.org/publicdomain/zero/1.0/ .

Source preview used: https://cdn.freesound.org/previews/531/531755_11537497-hq.mp3

Adaptation: mono PCM, 24 kHz, 16 bit; DC removed, short boundary fades, reduced gain, three subtly varied repeats with short pauses. Duration about 1.94 seconds; peak below -7 dBFS. No runtime download or decoder dependency is required. Windows controls playback volume and mute. Repeated clicks restart the clip rather than layering or queuing it. Timer sound remains a separate setting.

Playback uses Windows PlaySound with an asynchronous, process-lifetime memory buffer. Missing/muted audio output does not prevent the check-in or substitute a system beep.
