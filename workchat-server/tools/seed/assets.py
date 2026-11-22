"""Generates every sample file into a folder and writes manifest.json describing them (content type, details).

    python assets.py generated/      # (the seed script calls this itself when the folder has no manifest)
"""
import json
import sys
from pathlib import Path

import gen_docs
import gen_images
import gen_media
import people

CT = {
    "jpg": "image/jpeg", "png": "image/png", "mp4": "video/mp4", "m4a": "audio/mp4", "ogg": "audio/ogg",
    "pdf": "application/pdf", "txt": "text/plain", "csv": "text/csv", "zip": "application/zip",
    "docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
}


def build(outdir: Path):
    outdir.mkdir(parents=True, exist_ok=True)
    files = outdir / "files"
    av = outdir / "avatars"
    files.mkdir(exist_ok=True)
    av.mkdir(exist_ok=True)
    manifest = {"files": {}, "avatars": {}}

    def add(name, **meta):
        ext = name.rsplit(".", 1)[1]
        p = files / name
        manifest["files"][name] = {"path": str(p), "content_type": CT[ext], "size": p.stat().st_size, "meta": {k: v for k, v in meta.items() if v is not None}}

    print("photos ...")
    gen_images.save_photos(files)
    from PIL import Image
    for n in gen_images.PHOTOS:
        with Image.open(files / n) as im:
            add(n, width=im.width, height=im.height)

    print("videos ...")
    for n in gen_media.VIDEOS:
        p = gen_media.make_video(files, n)
        w, h, ms = gen_media.probe(p)
        poster, _ = gen_media.poster_b64(p)
        add(n, width=w, height=h, duration_ms=ms, poster_b64=poster)

    print("voice notes ...")
    for n, (sec, seed, hz) in gen_media.VOICE_NOTES.items():
        p = gen_media.make_voice(files, n, sec, seed, hz)
        _, _, ms = gen_media.probe(p)
        add(n, duration_ms=ms or int(sec * 1000), peaks=gen_media.peaks_of(p))

    print("documents ...")
    gen_docs.docx_minutes(files / "Meeting_Minutes_2026-11-14.docx")
    gen_docs.docx_policy(files / "Remote_Work_Policy_v3.docx")
    gen_docs.docx_contract(files / "Acme_MSA_Draft_v4.docx")
    gen_docs.xlsx_budget(files / "Budget_FY27_Draft.xlsx")
    gen_docs.xlsx_pipeline(files / "Q4_Sales_Pipeline.xlsx")
    gen_docs.xlsx_headcount(files / "Headcount_Plan_FY27.xlsx")
    n_q3 = gen_docs.pptx_q3(files / "Q3_Business_Review.pptx")
    n_brand = gen_docs.pptx_brand(files / "Brand_Guidelines_v2.1.pptx")
    pages = {
        "Q3_Workplace_Communications_Audit.pdf": gen_docs.pdf_audit(files / "Q3_Workplace_Communications_Audit.pdf"),
        "Incident_Postmortem_INC-2041.pdf": gen_docs.pdf_postmortem(files / "Incident_Postmortem_INC-2041.pdf"),
        "Invoice_INV-20817.pdf": gen_docs.pdf_invoice(files / "Invoice_INV-20817.pdf"),
    }
    gen_docs.write_txt(files)
    gen_docs.write_csv(files)
    gen_docs.write_zip(files, files)
    for n in ("Meeting_Minutes_2026-11-14.docx", "Remote_Work_Policy_v3.docx", "Acme_MSA_Draft_v4.docx", "Budget_FY27_Draft.xlsx", "Q4_Sales_Pipeline.xlsx", "Headcount_Plan_FY27.xlsx",
              "standup-notes-2026-11-19.txt", "release-notes-v3.1.0.txt", "customers-export-2026-11.csv", "brand-assets-v2.1.zip"):
        add(n)
    add("Q3_Business_Review.pptx", pages=n_q3)
    add("Brand_Guidelines_v2.1.pptx", pages=n_brand)
    for n, pg in pages.items():
        add(n, pages=pg)

    print("avatars ...")
    for username, _name, _title, _phone, look in people.PEOPLE:
        if look:
            skin, hair, style, shirt, bg, glasses, beard = look
            gen_images.person_avatar(av / f"{username}.png", skin, hair, style, shirt, bg, glasses, beard)
            manifest["avatars"][username] = str(av / f"{username}.png")
    for name, _topic, icon, _m in people.CHANNELS:
        gen_images.icon_tile(av / f"channel-{name}.png", icon)
        manifest["avatars"]["channel:" + name] = str(av / f"channel-{name}.png")
    gen_images.icon_tile(av / "group-launch-war-room.png", people.GROUP[1], colors=((30, 91, 184), (25, 60, 130)))
    manifest["avatars"]["channel:" + people.GROUP[0]] = str(av / "group-launch-war-room.png")
    for username, _n, _t, _d, icon, _c in people.BOTS:
        gen_images.icon_tile(av / f"bot-{username}.png", icon, colors=((25, 150, 110), (16, 100, 80)))
        manifest["avatars"][username] = str(av / f"bot-{username}.png")

    (outdir / "manifest.json").write_text(json.dumps(manifest, indent=1))
    total = sum(f["size"] for f in manifest["files"].values())
    print(f"{len(manifest['files'])} files, {total / 1e6:.1f} MB, {len(manifest['avatars'])} pictures -> {outdir}")
    return manifest


if __name__ == "__main__":
    build(Path(sys.argv[1] if len(sys.argv) > 1 else "generated"))
