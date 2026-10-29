"""Builds unofficial_sjk_extras.pk3 from six pk3s in GameData/base.

Usage: python -I build_extras.py <GameData/base> <out.pk3>

Files at the same path (case-insensitive) are taken once: byte-identical copies are
merged, and where they differ the first pack in SOURCES wins (Anim_Pack_v2's emotes,
which play on emote_rig2). Paths in DROP are left out because nothing refers to them
once Anim_Pack_v2's emotes win. Every .emote's rig, sounds and icon must be in the pack.
"""
import hashlib
import json
import sys
import zipfile
from pathlib import Path

SOURCES = [
    "Anim_Pack_v2.pk3",
    "nostress_emotes_fixed.pk3",
    "skinpack.pk3",
    "zz_solfox_voice_local.pk3",
    "zz_sjk_box_sounds_local.pk3",
    # The mystery box's laugh when a box blows up (sound/sjk/mystery_box/laugh.mp3), and
    # since 30/10/2026 its opening, roll and closing (extras_open, extras_roll and
    # extras_close.mp3 there).
    "zz_sjk_mystery_box_sounds_local.pk3",
]
DROP_PREFIXES = ("models/players/emote_rig/",)
DROP = {"sound/emotes/comrade.mp3"}

base, out = Path(sys.argv[1]), Path(sys.argv[2])
taken = {}  # lower path -> (source, name, sha)
merged, replaced, dropped = 0, [], []
plan = []
for source in SOURCES:
    with zipfile.ZipFile(base / source) as z:
        for info in z.infolist():
            if info.is_dir():
                continue
            key = info.filename.lower()
            if key in DROP or key.startswith(DROP_PREFIXES):
                dropped.append(info.filename)
                continue
            sha = hashlib.sha256(z.read(info)).hexdigest()
            if key in taken:
                if taken[key][2] == sha:
                    merged += 1
                else:
                    replaced.append((info.filename, source, taken[key][0]))
                continue
            taken[key] = (source, info.filename, sha)
            plan.append((source, info.filename))

names = {k for k in taken}
missing = []
for source, name in plan:
    if name.lower().endswith(".emote"):
        with zipfile.ZipFile(base / source) as z:
            emote = json.loads(z.read(name))
        rig = f"models/players/{emote['rig']}/sjk_rig.cfg".lower()
        refs = [rig] + [emote[k].lower() for k in ("sound", "sound_loop", "icon") if k in emote]
        missing += [(name, r) for r in refs if r not in names]
if missing:
    sys.exit(f"missing references: {missing}")

tmp = out.with_suffix(".tmp")
with zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as dst:
    for source in SOURCES:
        with zipfile.ZipFile(base / source) as z:
            for s, name in plan:
                if s == source:
                    data = z.read(name)
                    method = zipfile.ZIP_STORED if name.lower().endswith((".png", ".jpg", ".mp3", ".ogg")) else zipfile.ZIP_DEFLATED
                    info = zipfile.ZipInfo(name, date_time=z.getinfo(name).date_time)
                    info.compress_type = method
                    info.external_attr = 0o644 << 16
                    dst.writestr(info, data)
tmp.replace(out)

print(f"files {len(plan)}, identical copies merged {merged}, dropped {len(dropped)}")
for name, loser, winner in replaced:
    print(f"  kept {winner}'s {name} over {loser}'s")
digest = hashlib.md5(out.read_bytes()).hexdigest()
print(f"{out} {out.stat().st_size} bytes, md5 {digest}")
