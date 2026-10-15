-- Create admin user
INSERT INTO users (id, language, email, tier, defaultItemName, defaultHourlyRate) VALUES (
    '0000000000',
    'en',
    'admin@ksefbot.pl',
    0,
    'Test services',
    160
);
--companyLogo: X'0000010001002020000001002000A8040000160000002800000020000000400000000100200000000000000000000000000000000000000000000000000029B880FF2AB880FF2BB880FF2CB980FF2DB980FF2EB980FF2FB980FF30BA80FF31BA80FF32BA80FF',

-- Create the associated contractor data
INSERT INTO contractors (id, ownerId, name, nip, addressL1, notes) VALUES (
    '0000000000',
    '0000000000',
    'Test company',
    '0000000000',
    'Test city, 00-000, 0/0',
    'seller'
);
