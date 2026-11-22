"""Real, valid office documents and PDFs (python-docx, openpyxl, python-pptx, reportlab). Page counts are returned for the
attachment details the apps show on document cards."""
import csv
import random
import zipfile
from pathlib import Path

NAVY = "21375B"


# ------------------------------------------------------------------ DOCX

def docx_minutes(path: Path):
    from docx import Document
    from docx.shared import Pt, RGBColor
    d = Document()
    d.core_properties.title = "Meeting minutes 14 Nov 2026"
    d.core_properties.author = "WorkChat sample data"
    h = d.add_heading("Product Operations Weekly - Minutes", 0)
    d.add_paragraph("Date: Friday 14 November 2026, 10:00-10:45 (Perth)   Location: Meeting A / video")
    d.add_heading("Attendees", 1)
    t = d.add_table(rows=1, cols=3)
    t.style = "Light Grid Accent 1"
    for i, c in enumerate(("Name", "Role", "Present")):
        t.rows[0].cells[i].text = c
    for row in [("Katherine Wells", "Head of Product Operations", "Yes"), ("Alex Rivera", "Product Manager", "Yes"), ("Priya Patel", "Engineering Manager", "Yes"), ("Elena Vance", "VP Design", "Apologies"), ("Sarah Mitchell", "Director of Finance", "Yes")]:
        cells = t.add_row().cells
        for i, c in enumerate(row):
            cells[i].text = c
    d.add_heading("Decisions", 1)
    for s in ("Ship the thread list and read receipts in v1.1.0; voice notes stay in the next milestone.", "Keep the 100 MB attachment limit; revisit after the storage cost review.", "Freeze the navy theme tokens until the accessibility audit is finished."):
        d.add_paragraph(s, style="List Bullet")
    d.add_heading("Action items", 1)
    t = d.add_table(rows=1, cols=3)
    t.style = "Light Grid Accent 1"
    for i, c in enumerate(("Owner", "Action", "Due")):
        t.rows[0].cells[i].text = c
    for row in [("Alex", "Write release notes for v1.1.0", "Tue 18 Nov"), ("Priya", "Load test the files list with 50k attachments", "Thu 20 Nov"), ("Sarah", "Confirm Q4 hosting budget", "Fri 21 Nov"), ("Katherine", "Book the all-hands room for 3 Dec", "Mon 17 Nov")]:
        cells = t.add_row().cells
        for i, c in enumerate(row):
            cells[i].text = c
    p = d.add_paragraph()
    r = p.add_run("Next meeting: Friday 21 November, 10:00.")
    r.italic = True
    r.font.color.rgb = RGBColor(0x1E, 0x5B, 0xB8)
    d.save(path)


def docx_policy(path: Path):
    from docx import Document
    d = Document()
    d.core_properties.title = "Remote Work Policy v3"
    d.add_heading("Remote Work Policy", 0)
    d.add_paragraph("Version 3.0 - effective 1 December 2026. Owner: People & Culture.")
    sections = [
        ("Purpose", "This policy explains when and how employees may work away from the office, and what the company provides to make that work safe, productive and fair."),
        ("Eligibility", "Everyone whose role does not require physical presence may work remotely up to three days a week. Managers agree a rhythm with each team so that core collaboration days overlap."),
        ("Equipment and security", "The company supplies a laptop, a monitor allowance and a headset. Company data stays on managed devices. Use the VPN on public networks and lock the screen whenever you step away."),
        ("Working hours", "Core hours are 10:00 to 15:00 local time. Outside core hours, people are not expected to respond to messages unless they are on call."),
    ]
    for i, (h, body) in enumerate(sections, 1):
        d.add_heading(f"{i}. {h}", 1)
        d.add_paragraph(body)
    d.add_heading("Checklist for managers", 1)
    for s in ("Agree the weekly rhythm in writing", "Check equipment arrived within five working days", "Hold a monthly one-to-one that is not about tasks"):
        d.add_paragraph(s, style="List Number")
    d.save(path)


def docx_contract(path: Path):
    from docx import Document
    d = Document()
    d.core_properties.title = "Master Services Agreement - Acme Corp (draft)"
    d.add_heading("Master Services Agreement", 0)
    d.add_paragraph("DRAFT v4 - for review by Legal. Between WorkChat Pty Ltd (Supplier) and Acme Corporation (Customer).")
    clauses = [
        ("Services", "The Supplier provides the hosted team-chat service described in Schedule 1 and the support described in Schedule 2."),
        ("Term", "The agreement starts on the Effective Date and continues for 24 months, renewing for 12-month periods unless either party gives 90 days' notice."),
        ("Fees", "The Customer pays the fees in Schedule 3 monthly in advance. Invoices are payable within 30 days. Fees may rise once a year by no more than the CPI plus 2%."),
        ("Data protection", "The Supplier processes Customer data only on the Customer's instructions, keeps it in the agreed region and deletes it within 30 days of termination."),
        ("Liability", "Each party's total liability in a contract year is limited to the fees paid in the preceding 12 months, except where the law does not allow a limit."),
    ]
    for i, (h, body) in enumerate(clauses, 1):
        d.add_heading(f"{i}. {h}", 2)
        d.add_paragraph(body)
    d.add_paragraph("\nSigned for the Supplier: ______________________    Date: ____________")
    d.add_paragraph("Signed for the Customer: _____________________    Date: ____________")
    d.save(path)


# ------------------------------------------------------------------ XLSX

def xlsx_budget(path: Path):
    from openpyxl import Workbook
    from openpyxl.chart import BarChart, Reference
    from openpyxl.styles import Alignment, Font, PatternFill
    wb = Workbook()
    ws = wb.active
    ws.title = "By department"
    head = ["Department", "Q1", "Q2", "Q3", "Q4", "FY total"]
    ws.append(head)
    rows = [("Engineering", 420000, 435000, 450000, 468000), ("Design", 130000, 132000, 140000, 144000), ("Sales", 210000, 225000, 240000, 262000), ("Operations", 95000, 98000, 101000, 104000), ("People & Culture", 60000, 62000, 64000, 66000), ("Legal", 48000, 52000, 50000, 55000)]
    for i, r in enumerate(rows, 2):
        ws.append([*r, f"=SUM(B{i}:E{i})"])
    n = len(rows) + 1
    ws.append(["Total", *[f"=SUM({c}2:{c}{n})" for c in "BCDEF"]])
    for c in ws[1]:
        c.font = Font(bold=True, color="FFFFFF")
        c.fill = PatternFill("solid", fgColor=NAVY)
        c.alignment = Alignment(horizontal="center")
    for row in ws.iter_rows(min_row=2, min_col=2, max_col=6):
        for c in row:
            c.number_format = '"$"#,##0'
    for c in ws[n + 1]:
        c.font = Font(bold=True)
    ws.column_dimensions["A"].width = 22
    for col in "BCDEF":
        ws.column_dimensions[col].width = 14
    ws.freeze_panes = "B2"
    ch = BarChart()
    ch.type = "col"
    ch.title = "Spend by department (FY27)"
    ch.add_data(Reference(ws, min_col=6, min_row=1, max_row=n), titles_from_data=True)
    ch.set_categories(Reference(ws, min_col=1, min_row=2, max_row=n))
    ch.height, ch.width = 8, 16
    ws.add_chart(ch, "H2")
    s = wb.create_sheet("Summary", 0)
    s["A1"], s["A1"].font = "FY27 budget - draft", Font(bold=True, size=16, color=NAVY)
    s["A3"], s["B3"] = "Total spend", f"='By department'!F{n + 1}"
    s["A4"], s["B4"] = "Revenue plan", 6400000
    s["A5"], s["B5"] = "Operating margin", "=1-B3/B4"
    s["B3"].number_format = s["B4"].number_format = '"$"#,##0'
    s["B5"].number_format = "0.0%"
    s.column_dimensions["A"].width = 24
    s.column_dimensions["B"].width = 18
    wb.save(path)


def xlsx_pipeline(path: Path):
    from openpyxl import Workbook
    from openpyxl.styles import Font, PatternFill
    wb = Workbook()
    ws = wb.active
    ws.title = "Q4 pipeline"
    ws.append(["Account", "Owner", "Stage", "Value (USD)", "Probability", "Weighted", "Close date"])
    rnd = random.Random(7)
    accounts = ["Acme Corp", "Globex", "Initech", "Umbrella Health", "Stark Logistics", "Wayne Retail", "Hooli", "Pied Piper", "Soylent Foods", "Vandelay Imports", "Tyrell Labs", "Cyberdyne Systems"]
    stages = [("Discovery", 0.15), ("Demo", 0.3), ("Proposal", 0.5), ("Negotiation", 0.75), ("Closed won", 1.0)]
    owners = ["Marcus Brody", "Sarah Mitchell", "Alex Rivera"]
    for i, a in enumerate(accounts, 2):
        st, p = rnd.choice(stages)
        ws.append([a, rnd.choice(owners), st, rnd.randrange(12, 180) * 1000, p, f"=D{i}*E{i}", f"2026-{rnd.choice([11, 12])}-{rnd.randint(1, 28):02d}"])
    n = len(accounts) + 1
    ws.append(["Total", "", "", f"=SUM(D2:D{n})", "", f"=SUM(F2:F{n})", ""])
    for c in ws[1]:
        c.font = Font(bold=True, color="FFFFFF")
        c.fill = PatternFill("solid", fgColor="1E5BB8")
    for row in ws.iter_rows(min_row=2, max_row=n + 1):
        row[3].number_format = '"$"#,##0'
        row[4].number_format = "0%"
        row[5].number_format = '"$"#,##0'
    for col, w in zip("ABCDEFG", (22, 18, 14, 14, 12, 14, 14)):
        ws.column_dimensions[col].width = w
    ws.append([])
    ws[n + 1][0].font = Font(bold=True)
    wb.save(path)


def xlsx_headcount(path: Path):
    from openpyxl import Workbook
    from openpyxl.styles import Font, PatternFill
    wb = Workbook()
    ws = wb.active
    ws.title = "Headcount"
    ws.append(["Team", "Current", "Planned hires", "Attrition (est.)", "End of FY27"])
    for i, (t, cur, hire, att) in enumerate([("Engineering", 38, 8, 3), ("Design", 9, 2, 0), ("Product", 7, 1, 1), ("Sales", 14, 5, 2), ("Operations", 6, 1, 0), ("People & Culture", 4, 1, 0), ("Legal & Finance", 5, 0, 0)], 2):
        ws.append([t, cur, hire, att, f"=B{i}+C{i}-D{i}"])
    ws.append(["Total", "=SUM(B2:B8)", "=SUM(C2:C8)", "=SUM(D2:D8)", "=SUM(E2:E8)"])
    for c in ws[1]:
        c.font = Font(bold=True, color="FFFFFF")
        c.fill = PatternFill("solid", fgColor=NAVY)
    for c in ws[9]:
        c.font = Font(bold=True)
    ws.column_dimensions["A"].width = 22
    wb.save(path)


# ------------------------------------------------------------------ PPTX

def _title_slide(prs, title, sub):
    s = prs.slides.add_slide(prs.slide_layouts[0])
    s.shapes.title.text = title
    s.placeholders[1].text = sub
    return s


def pptx_q3(path: Path):
    from pptx import Presentation
    from pptx.chart.data import CategoryChartData
    from pptx.dml.color import RGBColor
    from pptx.enum.chart import XL_CHART_TYPE
    from pptx.util import Inches, Pt
    prs = Presentation()
    prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)
    _title_slide(prs, "Q3 Business Review", "Executive Leadership - 12 November 2026")
    s = prs.slides.add_slide(prs.slide_layouts[1])
    s.shapes.title.text = "Agenda"
    tf = s.placeholders[1].text_frame
    tf.text = "Highlights and misses"
    for line in ("Revenue and margin", "Product milestones", "People and hiring", "Risks for Q4", "Decisions needed"):
        tf.add_paragraph().text = line
    s = prs.slides.add_slide(prs.slide_layouts[5])
    s.shapes.title.text = "Revenue by quarter ($M)"
    cd = CategoryChartData()
    cd.categories = ["Q1", "Q2", "Q3", "Q4 (plan)"]
    cd.add_series("FY26", (1.1, 1.3, 1.45, 1.6))
    cd.add_series("FY27", (1.4, 1.6, 1.8, 2.1))
    s.shapes.add_chart(XL_CHART_TYPE.COLUMN_CLUSTERED, Inches(1), Inches(1.6), Inches(11), Inches(5.2), cd)
    s = prs.slides.add_slide(prs.slide_layouts[5])
    s.shapes.title.text = "Key metrics"
    rows = [("Metric", "Q2", "Q3", "Change"), ("Active users", "7,950", "9,482", "+19%"), ("Net revenue retention", "108%", "112%", "+4 pts"), ("Support first response", "41 min", "29 min", "-29%"), ("Uptime", "99.93%", "99.97%", "+0.04 pts")]
    tbl = s.shapes.add_table(len(rows), 4, Inches(1), Inches(1.8), Inches(11), Inches(3.4)).table
    for r, row in enumerate(rows):
        for c, v in enumerate(row):
            tbl.cell(r, c).text = v
    s = prs.slides.add_slide(prs.slide_layouts[1])
    s.shapes.title.text = "Q4 risks"
    tf = s.placeholders[1].text_frame
    tf.text = "Hosting costs rise with attachment volume"
    tf.add_paragraph().text = "Two senior engineering hires still open"
    tf.add_paragraph().text = "Holiday freeze shortens the release window"
    _title_slide(prs, "Questions?", "Thank you")
    prs.save(path)
    return len(prs.slides)


def pptx_brand(path: Path):
    from pptx import Presentation
    from pptx.dml.color import RGBColor
    from pptx.enum.shapes import MSO_SHAPE
    from pptx.util import Inches, Pt
    prs = Presentation()
    prs.slide_width, prs.slide_height = Inches(13.333), Inches(7.5)
    _title_slide(prs, "Brand Guidelines v2.1", "Design team - colour, type and voice")
    s = prs.slides.add_slide(prs.slide_layouts[5])
    s.shapes.title.text = "Colour palette"
    for i, (name, hexv) in enumerate([("Navy Classic", "21375B"), ("Deep Navy", "19273F"), ("Link Blue", "1E5BB8"), ("Slate", "4A5F86"), ("Mint", "19C37D")]):
        x = Inches(0.8 + i * 2.4)
        sh = s.shapes.add_shape(MSO_SHAPE.ROUNDED_RECTANGLE, x, Inches(2), Inches(2.1), Inches(2.6))
        sh.fill.solid()
        sh.fill.fore_color.rgb = RGBColor.from_string(hexv)
        sh.line.fill.background()
        tb = s.shapes.add_textbox(x, Inches(4.8), Inches(2.1), Inches(0.9))
        tb.text_frame.text = f"{name}\n#{hexv}"
    s = prs.slides.add_slide(prs.slide_layouts[1])
    s.shapes.title.text = "Typography"
    tf = s.placeholders[1].text_frame
    tf.text = "Inter for all UI text; 16 pt default message size"
    tf.add_paragraph().text = "Headings in semibold, never all caps except section labels"
    tf.add_paragraph().text = "Numbers use tabular figures in tables and counters"
    s = prs.slides.add_slide(prs.slide_layouts[1])
    s.shapes.title.text = "Voice and tone"
    tf = s.placeholders[1].text_frame
    tf.text = "Plain, direct, kind"
    tf.add_paragraph().text = "Say what happened and what to do next"
    tf.add_paragraph().text = "Menu labels are verbs: Lock, Unlock, Pin, Forward"
    _title_slide(prs, "Thank you", "Questions to #design-system")
    prs.save(path)
    return len(prs.slides)


# ------------------------------------------------------------------ PDF

def _pdf(path: Path, title: str, subtitle: str, pages: int, seed: int, kind: str = "report"):
    from reportlab.lib import colors
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
    from reportlab.lib.units import mm
    from reportlab.graphics.charts.barcharts import VerticalBarChart
    from reportlab.graphics.shapes import Drawing
    from reportlab.platypus import BaseDocTemplate, Frame, PageBreak, PageTemplate, Paragraph, Spacer, Table, TableStyle, KeepTogether
    rnd = random.Random(seed)
    navy = colors.HexColor("#21375B")
    ss = getSampleStyleSheet()
    h1 = ParagraphStyle("h1", parent=ss["Heading1"], textColor=navy, fontSize=20, spaceAfter=10)
    h2 = ParagraphStyle("h2", parent=ss["Heading2"], textColor=navy, fontSize=14, spaceBefore=8)
    body = ParagraphStyle("b", parent=ss["BodyText"], fontSize=10.5, leading=15)
    words = ("communication channel team message update workflow review policy clarity response adoption notification thread document owner quarter "
             "survey result trend insight leadership alignment process feedback measure target baseline improvement risk mitigation action").split()

    def para(n=70):
        s = " ".join(rnd.choice(words) for _ in range(n))
        return Paragraph(s.capitalize() + ".", body)

    def on_page(c, d):
        c.saveState()
        c.setFillColor(navy)
        c.rect(0, A4[1] - 12 * mm, A4[0], 12 * mm, stroke=0, fill=1)
        c.setFillColor(colors.white)
        c.setFont("Helvetica-Bold", 9)
        c.drawString(18 * mm, A4[1] - 8 * mm, title)
        c.setFillColor(colors.grey)
        c.setFont("Helvetica", 8)
        c.drawRightString(A4[0] - 18 * mm, 10 * mm, f"Page {d.page}")
        c.restoreState()

    doc = BaseDocTemplate(str(path), pagesize=A4, title=title, author="WorkChat sample data")
    doc.addPageTemplates([PageTemplate(id="p", frames=[Frame(18 * mm, 18 * mm, A4[0] - 36 * mm, A4[1] - 40 * mm)], onPage=on_page)])
    story = [Spacer(1, 50 * mm), Paragraph(title, ParagraphStyle("t", parent=h1, fontSize=30, leading=36)), Paragraph(subtitle, ParagraphStyle("s", parent=body, fontSize=14, textColor=colors.grey)), PageBreak()]
    for pg in range(2, pages + 1):
        story.append(Paragraph(f"{pg - 1}. " + rnd.choice(["Overview", "Method", "Findings", "Channel usage", "Meeting load", "Response times", "Recommendations", "Appendix"]), h1))
        story.append(para(90))
        if pg % 3 == 0:
            d = Drawing(400, 170)
            bc = VerticalBarChart()
            bc.x, bc.y, bc.width, bc.height = 30, 20, 340, 130
            bc.data = [[rnd.randint(20, 95) for _ in range(6)], [rnd.randint(20, 95) for _ in range(6)]]
            bc.categoryAxis.categoryNames = ["Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
            bc.bars[0].fillColor, bc.bars[1].fillColor = navy, colors.HexColor("#1E5BB8")
            d.add(bc)
            story += [Spacer(1, 6), d, Spacer(1, 6)]
        if pg % 4 == 0:
            data = [["Team", "Messages / day", "Avg reply (min)", "Satisfaction"]] + [[t, str(rnd.randint(300, 4000)), str(rnd.randint(2, 40)), f"{rnd.uniform(3.2, 4.8):.1f}/5"] for t in ("Engineering", "Design", "Sales", "Operations", "Leadership")]
            t = Table(data, colWidths=[45 * mm, 40 * mm, 40 * mm, 35 * mm])
            t.setStyle(TableStyle([("BACKGROUND", (0, 0), (-1, 0), navy), ("TEXTCOLOR", (0, 0), (-1, 0), colors.white), ("GRID", (0, 0), (-1, -1), 0.4, colors.lightgrey), ("ROWBACKGROUNDS", (0, 1), (-1, -1), [colors.white, colors.HexColor("#F4F6FA")]), ("FONTSIZE", (0, 0), (-1, -1), 9)]))
            story += [Spacer(1, 8), t, Spacer(1, 8)]
        story.append(Paragraph(rnd.choice(["What we saw", "Why it matters", "Next steps"]), h2))
        story.append(para(70))
        story.append(PageBreak())
    story.pop()
    doc.build(story)
    return pages


def pdf_audit(path: Path):
    return _pdf(path, "Q3 Workplace Communications Audit", "Prepared by Product Operations - November 2026", 28, 1)


def pdf_postmortem(path: Path):
    return _pdf(path, "Incident postmortem INC-2041", "Attachment storage latency, 9 November 2026", 5, 2)


def pdf_invoice(path: Path):
    from reportlab.lib import colors
    from reportlab.lib.pagesizes import A4
    from reportlab.lib.units import mm
    from reportlab.pdfgen import canvas
    c = canvas.Canvas(str(path), pagesize=A4)
    c.setTitle("Invoice INV-20817")
    c.setFillColor(colors.HexColor("#21375B"))
    c.rect(0, 270 * mm, 210 * mm, 27 * mm, stroke=0, fill=1)
    c.setFillColor(colors.white)
    c.setFont("Helvetica-Bold", 22)
    c.drawString(20 * mm, 280 * mm, "INVOICE  INV-20817")
    c.setFillColor(colors.black)
    c.setFont("Helvetica", 10)
    y = 255 * mm
    for line in ("Blue Heron Studio Pty Ltd", "ABN 51 824 753 556", "14 Mill Point Rd, South Perth WA 6151", "", "Bill to: WorkChat Pty Ltd - Accounts Payable", "Issue date: 12 Nov 2026     Due: 12 Dec 2026"):
        c.drawString(20 * mm, y, line)
        y -= 6 * mm
    y -= 6 * mm
    c.setFont("Helvetica-Bold", 10)
    for x, h in ((20, "Description"), (120, "Qty"), (140, "Unit"), (170, "Amount")):
        c.drawString(x * mm, y, h)
    c.line(20 * mm, y - 2 * mm, 190 * mm, y - 2 * mm)
    c.setFont("Helvetica", 10)
    total = 0
    for desc, qty, unit in (("Brand illustration set (12 pieces)", 1, 2400), ("Icon pack - navigation and chat", 1, 1350), ("Workshop day - design system", 2, 1100), ("Stock licence (extended)", 1, 180)):
        y -= 8 * mm
        c.drawString(20 * mm, y, desc)
        c.drawString(120 * mm, y, str(qty))
        c.drawString(140 * mm, y, f"${unit:,.2f}")
        c.drawString(170 * mm, y, f"${qty * unit:,.2f}")
        total += qty * unit
    y -= 12 * mm
    c.setFont("Helvetica-Bold", 11)
    c.drawString(140 * mm, y, "Total (AUD)")
    c.drawString(170 * mm, y, f"${total:,.2f}")
    c.setFont("Helvetica", 9)
    c.drawString(20 * mm, 30 * mm, "Pay by bank transfer: BSB 306-069  Account 1048 2276  Reference INV-20817")
    c.save()
    return 1


# ------------------------------------------------------------------ plain files

def write_txt(outdir: Path):
    (outdir / "standup-notes-2026-11-19.txt").write_text(
        "Standup 19 Nov 2026\n===================\n\nYesterday\n- Priya: finished the files list endpoint, added paging.\n- Omar: rotated the staging certificates.\n- David: fixed the video poster bug on Android.\n\n"
        "Today\n- Priya: review the thread list query plan.\n- Omar: move the nightly backup to 02:00.\n- David: voice note waveform on the phone layout.\n\nBlockers\n- Waiting on the signing key for the iOS build (Alex).\n", encoding="utf-8")
    (outdir / "release-notes-v3.1.0.txt").write_text(
        "WorkChat 3.1.0 - release notes\n\nNew\n * Threads tab with unread counts\n * Read receipts (double ticks turn blue when read)\n * Files tab: filter by photos, videos, audio and documents\n\nImproved\n * Faster first sync on slow networks\n * Voice notes show a waveform\n\nFixed\n * Rare duplicate message after a reconnect\n * Pinned messages sometimes hidden in group chats\n", encoding="utf-8")


def write_csv(outdir: Path):
    rnd = random.Random(12)
    with open(outdir / "customers-export-2026-11.csv", "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["id", "company", "plan", "seats", "mrr_usd", "renewal"])
        for i, c in enumerate(["Acme Corp", "Globex", "Initech", "Umbrella Health", "Stark Logistics", "Wayne Retail", "Hooli", "Soylent Foods"], 1):
            seats = rnd.randrange(20, 400, 5)
            w.writerow([1000 + i, c, rnd.choice(["Team", "Business", "Enterprise"]), seats, seats * rnd.choice([6, 9, 12]), f"2027-{rnd.randint(1, 12):02d}-01"])


def write_zip(outdir: Path, imgdir: Path):
    with zipfile.ZipFile(outdir / "brand-assets-v2.1.zip", "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("README.txt", "Brand assets v2.1\n\nlogo/ has the app icon, colors/ the palette. Use the navy tile on light backgrounds.\n")
        z.writestr("colors/palette.json", '{"navy":"#21375B","deepNavy":"#19273F","link":"#1E5BB8","slate":"#4A5F86","page":"#F4F6FA"}\n')
        for n in ("brand-moodboard.jpg", "product-mockup.jpg"):
            if (imgdir / n).exists():
                z.write(imgdir / n, f"images/{n}")
