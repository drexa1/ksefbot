DROP TABLE IF EXISTS users;
CREATE TABLE users (
    -- Identification data
    id TEXT PRIMARY KEY CHECK (length(id) = 10 AND id NOT GLOB '*[^0-9]*'),  -- Users PK: NIP
    language TEXT NOT NULL CHECK (language IN ('en', 'pl')),
    phone TEXT UNIQUE CHECK (phone GLOB '+[0-9]*' AND length(phone) BETWEEN 8 AND 15 AND phone NOT GLOB '*[^+0-9]*'),  -- E.164
    email TEXT UNIQUE,
    googleSubject TEXT UNIQUE,
    companyLogo BLOB,
    -- Contractor data
    contractorId TEXT REFERENCES contractors(id),
    -- Application
    tier INTEGER NOT NULL,
    apiKey TEXT UNIQUE,
    -- KSeF integration
    ksefApiToken TEXT UNIQUE,
    -- Invoicing defaults,
    defaultItemName TEXT,
    defaultHourlyRate INTEGER,
    settlementType TEXT NOT NULL CHECK (settlementType IN ('monthly', 'quarterly')) DEFAULT 'monthly',
    -- Banking integration
    bankName TEXT,
    bankAccountNumber TEXT CHECK (length(bankAccountNumber) = 26 AND bankAccountNumber NOT GLOB '*[^0-9]*'),
    bankApiToken TEXT UNIQUE,
    -- DBA
    createdAt TEXT DEFAULT CURRENT_TIMESTAMP,
    updatedAt TEXT
);

DROP TABLE IF EXISTS contractors;
CREATE TABLE contractors (
    id TEXT PRIMARY KEY,
    -- Owner
    ownerId TEXT NOT NULL REFERENCES users(id),
    -- Contractor data
    name TEXT NOT NULL,
    nip TEXT UNIQUE CHECK (length(nip) = 10 AND nip NOT GLOB '*[^0-9]*'),
    pesel TEXT UNIQUE CHECK (length(pesel) = 11 AND pesel NOT GLOB '*[^0-9]*'),
    regon TEXT UNIQUE CHECK ((length(regon) = 9 OR length(regon) = 14) AND regon NOT GLOB '*[^0-9]*'),  -- standard: 9 digits, unit/subunit: 14 digits
    internalIdentifier TEXT UNIQUE,
    -- Addresses
    addressL1 TEXT NOT NULL,
    addressL2 TEXT,
    countryCode TEXT NOT NULL DEFAULT 'PL',
    notes TEXT,
    -- JST/VAT group
    localGovernmentUnit INTEGER,
    vatGroup INTEGER,
    -- DBA
    createdAt TEXT DEFAULT CURRENT_TIMESTAMP,
    updatedAt TEXT
);

DROP TABLE IF EXISTS invoices;
CREATE TABLE invoices (
    id TEXT PRIMARY KEY,
    -- Owner
    ownerId TEXT NOT NULL REFERENCES users(id),
    -- Parties
    type TEXT NOT NULL CHECK (type IN ('sales', 'purchase')),
    customerId TEXT REFERENCES contractors(id),  -- Nullable if 'purchase'
    -- Raw data
    rawXml  TEXT NOT NULL,
    jsonData TEXT NOT NULL CHECK (json_valid(jsonData)),
    notes TEXT,
    -- DBA
    createdAt TEXT DEFAULT CURRENT_TIMESTAMP,
    updatedAt TEXT
);

DROP TABLE IF EXISTS taxes;
CREATE TABLE taxes (
    "from" DATE,
    "to" DATE,
    -- Owner
    ownerId TEXT NOT NULL REFERENCES users(id),
    -- Tax record
    brutIncome REAL NOT NULL,
    -- VAT
    vatPercentage REAL DEFAULT 23,
    vatAmount REAL NOT NULL,
    netBeforeObligations REAL NOT NULL,
    -- Obligations
    taxRate REAL DEFAULT 12,
    incomeTax REAL NOT NULL,
    healthInsuranceBase REAL DEFAULT 5537.18,
    healthInsuranceRate REAL DEFAULT 9,
    healthContribution REAL NOT NULL,
    -- Purchases deductions
    purchasesSummary TEXT CHECK (json_valid(purchasesSummary)),
    -- Total after obligations and purchases deductions
    totalCleanRevenue REAL NOT NULL,
    notes TEXT,
    -- DBA
    createdAt TEXT DEFAULT CURRENT_TIMESTAMP,
    updatedAt TEXT,
    PRIMARY KEY ("from", "to")
);