"""The sample company: people, channels, group chat, DMs and bots (names are invented)."""

# username, display name, title, phone, avatar look (None = initials only, no picture)
# look = (skin, hair, style, shirt, background gradient, glasses, beard)
PEOPLE = [
    ("katherine.wells", "Katherine Wells", "Head of Product Operations", "+1 (555) 382-9014",
     ((236, 190, 160), (74, 48, 36), "long", (33, 55, 91), ((214, 228, 255), (170, 196, 240)), False, False)),
    ("david.kim", "David Kim", "Staff Engineer", "+1 (555) 301-2277",
     ((222, 178, 140), (28, 26, 30), "short", (60, 70, 90), ((200, 232, 224), (150, 205, 190)), True, False)),
    ("marcus.brody", "Marcus Brody", "Senior Account Executive", "+1 (555) 301-4410",
     ((120, 80, 58), (22, 18, 18), "buzz", (30, 91, 184), ((250, 226, 196), (240, 190, 140)), False, True)),
    ("priya.patel", "Priya Patel", "Engineering Manager", "+1 (555) 301-8832",
     ((196, 140, 104), (24, 18, 20), "bun", (120, 52, 92), ((236, 218, 250), (205, 180, 240)), False, False)),
    ("liam.chen", "Liam Chen", "Legal Counsel", "+1 (555) 301-5519", None),
    ("elena.vance", "Elena Vance", "VP Design", "+1 (555) 301-7743",
     ((244, 208, 180), (176, 96, 48), "bob", (40, 40, 52), ((252, 224, 224), (240, 180, 180)), False, False)),
    ("sarah.mitchell", "Sarah Mitchell", "Director of Finance", "+1 (555) 301-6602",
     ((248, 216, 190), (214, 178, 100), "long", (24, 120, 98), ((222, 240, 214), (176, 216, 160)), True, False)),
    ("alex.rivera", "Alex Rivera", "Senior Product Manager", "+1 (555) 301-9087",
     ((205, 150, 112), (50, 34, 28), "curly", (200, 110, 60), ((255, 236, 204), (246, 200, 130)), False, False)),
    ("omar.haddad", "Omar Haddad", "DevOps Engineer", "+1 (555) 301-3358",
     ((176, 124, 90), (30, 24, 22), "short", (44, 52, 70), ((216, 226, 240), (160, 180, 214)), False, True)),
    ("grace.nguyen", "Grace Nguyen", "People & Culture Lead", "+1 (555) 301-2094", None),
]

USERNAMES = [p[0] for p in PEOPLE]
FIRST = {p[0]: p[1].split()[0] for p in PEOPLE}

# name, topic, icon, member usernames
CHANNELS = [
    ("company-announcements", "Company-wide news. Read-only for most people: use threads for questions.", "megaphone", USERNAMES),
    ("engineering-deployments", "Release trains, rollbacks and what shipped. Bots welcome.", "terminal",
     ["david.kim", "priya.patel", "omar.haddad", "alex.rivera", "katherine.wells"]),
    ("product-strategy", "Roadmap, bets and trade-offs. Q4 planning lives here.", "chart",
     ["katherine.wells", "alex.rivera", "elena.vance", "priya.patel", "sarah.mitchell", "david.kim"]),
    ("design-system", "Tokens, components, accessibility. Figma links welcome.", "palette",
     ["elena.vance", "alex.rivera", "david.kim", "katherine.wells"]),
    ("executive-leadership", "Leadership team. Keep it tidy, put decisions in the pinned notes.", "star",
     ["katherine.wells", "sarah.mitchell", "priya.patel", "elena.vance", "marcus.brody"]),
    ("sales-pipeline", "Deals, forecasts and customer asks.", "funnel",
     ["marcus.brody", "sarah.mitchell", "alex.rivera", "katherine.wells"]),
    ("people-culture", "Hiring, onboarding, events and policy questions.", "heart",
     ["grace.nguyen", "katherine.wells", "sarah.mitchell", "elena.vance", "priya.patel"]),
    ("legal-compliance", "Contracts, privacy reviews and audit requests.", "scale",
     ["liam.chen", "katherine.wells", "sarah.mitchell", "grace.nguyen"]),
    ("incident-response", "Production incidents. One thread per incident.", "alert",
     ["omar.haddad", "david.kim", "priya.patel", "katherine.wells"]),
    ("random-lunch", "Coffee, lunch plans and photos. Keep it kind.", "coffee", USERNAMES),
]

# a group DM created by the admin
GROUP = ("launch-war-room", "rocket", ["katherine.wells", "alex.rivera", "priya.patel", "elena.vance"])

# direct messages between two people (the first one starts it)
DMS = [
    ("katherine.wells", "david.kim"),
    ("katherine.wells", "marcus.brody"),
    ("katherine.wells", "priya.patel"),
    ("katherine.wells", "liam.chen"),
    ("katherine.wells", "elena.vance"),
    ("david.kim", "priya.patel"),
    ("alex.rivera", "elena.vance"),
    ("sarah.mitchell", "marcus.brody"),
]

# username, display name, title, description, icon, channels it is a member of
BOTS = [
    ("deploybot", "Deploy Bot", "CI/CD", "Posts deployments and pipeline results.", "rocket", ["engineering-deployments", "incident-response"]),
    ("remindbot", "Reminder Bot", "Calendar", "Posts reminders and the weekly calendar digest.", "calendar", ["company-announcements", "executive-leadership"]),
]

# the person whose phone the seed is mostly tuned for: favourite chats, unread counts
MAIN_USER = "katherine.wells"
FAVOURITES = ["executive-leadership", "product-strategy", "dm:elena.vance"]
MUTED = ["random-lunch"]
