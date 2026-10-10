import {XMLParser} from "fast-xml-parser";
import XMLBuilder from "fast-xml-builder";
import {getAuthUser} from "../auth";
import {D1Driver, Repository} from "../repository/d1";
import {AuthError} from "../types/auth";
import {AppContractor, KsefIdentifiable} from "../types/contractors";
import {AppInvoice, InvoiceInput} from "../types/invoices";
import {AppUser} from "../types/users";
import {Env} from "../worker";
import DateTimeFormat = Intl.DateTimeFormat;
import {dtoFromAliases} from "../dto/avro";
import {nanoid} from "nanoid";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

// ---------------------------------------------------------------------------------------------------------------------
// Application invoice model from XML
// ---------------------------------------------------------------------------------------------------------------------

const invoiceParser = new XMLParser({
    attributesGroupName: ":@",
    textNodeName: "value",
    attributeNamePrefix: "",
    removeNSPrefix: true,
    ignoreAttributes: false,
    parseTagValue: false
});

const round = (amount: number): number => Math.round(amount * 100) / 100;
const date = (value: Date): string => value.toISOString().slice(0, 10);

export async function invoiceFromXml(
    env: Env,
    xmlContent: string,
    appUser: AppUser,
    type: "sales" | "purchase",
    notes?: string
): Promise<AppInvoice & { ownerId: string }> {
    // Parse XML
    const invoiceXml = invoiceParser.parse(xmlContent).Faktura;
    const ksefInvoiceAvroSchema = await env.ASSETS.fetch(new URL(env.KSEF_INVOICE_SCHEMA)).then((res) => res.json());
    const ksefInvoice = dtoFromAliases(invoiceXml, ksefInvoiceAvroSchema);
    return {
        id: ksefInvoice.InvoiceBody.InvoiceNumber,
        ownerId: appUser.id,
        type: type,
        issueDate: ksefInvoice.InvoiceBody.IssueDate,
        // Only auto create customers for sales invoices
        ...(type === "sales" && {
            customerId: await getOrCreateContractor(env, {
                ownerId: appUser.id,
                name: ksefInvoice.Buyer.IdentificationData.Name,
                nip: ksefInvoice.Buyer.IdentificationData.NIP,
                countryCode: ksefInvoice.Buyer.Address?.CountryCode,
                addressL1: ksefInvoice.Buyer.Address?.AddressLine1,
            })
        }),
        rawXml: xmlContent,
        jsonData: JSON.stringify(ksefInvoice),
        ...(notes && { notes }),
        updatedAt: new Date().toISOString()
    };
}

async function getOrCreateContractor(env: Env, contractorParts: {
    ownerId: string
    name: string
    nip?: string
    pesel?: string
    regon?: string
    countryCode?: string
    addressL1?: string
}): Promise<string> {
    const repo = getRepo(env);
    const { idField, idValue } = getContractorIdentifier(contractorParts);
    const lookup = { [idField]: idValue };
    const existing = await repo.get<AppContractor>("contractors", lookup);
    if (existing) return existing.id!;
    const contractor: AppContractor = {
        id: nanoid(),
        ...({ ownerId: contractorParts.ownerId }),
        name: contractorParts.name,
        ...(contractorParts.nip && { nip: contractorParts.nip }),
        ...(contractorParts.pesel && { pesel: contractorParts.pesel }),
        ...(contractorParts.regon && { regon: contractorParts.regon }),
        countryCode: contractorParts.countryCode ?? "PL",
        addressL1: contractorParts.addressL1 ?? "",
        createdAt: new Date().toISOString(),
    };
    const { changes } = await repo.save("contractors", contractor, true);
    if (changes) return contractor.id!;
    const saved = await repo.get<AppContractor>("contractors", lookup);
    return saved.id;
}

function getContractorIdentifier(contractorId: KsefIdentifiable): { idField: "nip" | "pesel" | "regon", idValue: string } {
    if (contractorId.nip)
        return { idField: "nip", idValue: contractorId.nip };
    if (contractorId.pesel)
        return { idField: "pesel", idValue: contractorId.pesel };
    if (contractorId.regon)
        return { idField: "regon", idValue: contractorId.regon };
    throw new Error("Contractor without supported identifier");
}

// ---------------------------------------------------------------------------------------------------------------------
// XML content from application invoice model
// ---------------------------------------------------------------------------------------------------------------------

const templateParser = new XMLParser({ preserveOrder: true, ignoreAttributes: false, parseTagValue: false });
const xmlBuilder = new XMLBuilder({ preserveOrder: true, ignoreAttributes: false, format: true, suppressEmptyNode: true });

const children = (parent: any[], name: string): any[] => parent.find(node => name in node)[name];
const elements = (values: Record<string, string | number>): any[] => Object.entries(values).map(([name, value]) => ({ [name]: [{ "#text": value }] }));

export async function generate(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const input = await req.json() as InvoiceInput;
    const repo = getRepo(env);
    const seller = await repo.get<AppContractor>("contractors", {
        ...(appUser.contractorId ? { id: appUser.contractorId } : { nip: appUser.id }), ownerId: appUser.id
    });
    const customer = await repo.get<AppContractor>("contractors", { id: input.customerId, ownerId: appUser.id });
    if (!seller || !customer)
        throw new AuthError("Seller or customer not found", 404);
    const templateXml = await env.ASSETS.fetch(new URL("/schemas/invoice-template.xml", req.url)).then(res => res.text());
    return new Response(generateInvoiceXml(templateXml, input, appUser, seller, customer), {
        headers: { "Content-Type": "application/xml; charset=utf-8" }
    });
}

export function generateInvoiceXml(templateXml: string, input: InvoiceInput, appUser: AppUser, seller: AppContractor, customer: AppContractor): string {
    const document = templateParser.parse(templateXml);
    const root = children(document, "Faktura");
    const fa = children(root, "Fa");
    const sellerElement = children(root, "Podmiot1");
    const buyerElement = children(root, "Podmiot2");
    const annotations = children(fa, "Adnotacje");
    const payment = children(fa, "Platnosc");

    // Header
    setText(children(root, "Naglowek"), {
        KodFormularza: "FA",
        WariantFormularza: "3",
        SystemInfo: "KSeF Bot",
        DataWytworzeniaFa: new Date().toISOString()
    });

    // Seller
    setText(children(sellerElement, "DaneIdentyfikacyjne"), { NIP: seller.nip!, Nazwa: seller.name });
    setText(children(sellerElement, "Adres"), { AdresL1: seller.addressL1, KodKraju: seller.countryCode });

    // Customer
    const buyer = input.customer ?? customer;
    const buyerIdentification = children(buyerElement, "DaneIdentyfikacyjne");
    setText(buyerIdentification, { NIP: buyer.nip ?? "", Nazwa: buyer.name });
    if (!buyer.nip) buyerIdentification.splice(buyerIdentification.findIndex(node => "NIP" in node), 1, ...elements({ BrakID: 1 }));
    setText(children(buyerElement, "Adres"), { AdresL1: buyer.addressL1, KodKraju: buyer.countryCode });
    if (input.customerEmail)
        buyerElement.splice(buyerElement.findIndex(node => "JST" in node), 0, { DaneKontaktowe: elements({ Email: input.customerEmail }) });

    // Additional entity
    setText(buyerElement, {
        JST: input.additionalEntity === "jst" ? 1 : 2,
        GV: input.additionalEntity === "gv" ? 1 : 2
    });

    // Invoice
    if (!input.items && (!Number.isInteger(input.hours) || input.hours! <= 0))
        throw new AuthError("Hours must be a positive integer", 400);
    if (!input.items && (!Number.isFinite(appUser.defaultHourlyRate) || appUser.defaultHourlyRate! <= 0))
        throw new AuthError("Default hourly rate is not configured", 400);

    const issueDate = input.issueDate ?? new DateTimeFormat("en").format(new Date());
    const issuePlace = input.issuePlace ?? seller.addressL1.split(",")[0].trim();
    const [year, month] = issueDate.split("-").map(Number);
    const postingDate = input.postingDate ?? date(new Date(Date.UTC(year, month, 0)));
    const items: NonNullable<InvoiceInput["items"]> = input.items ?? [{
        name: appUser.defaultItemName?.trim() || "Consulting services",
        quantity: input.hours!, unit: "szt", unitPrice: appUser.defaultHourlyRate!, vatRate: "23"
    }];
    const lines = items.map(item => {
        const rate = item.vatRate === "ZW" ? 0 : Number(item.vatRate);
        const amount = item.unitPrice * item.quantity;
        const net = round(input.priceType === "gross" ? amount / (1 + rate / 100) : amount);
        const vat = input.priceType === "gross" ? round(round(amount) - net) : round(amount * rate / 100);
        return { ...item, net, vat, gross: round(net + vat) };
    });
    const totals = Object.fromEntries((["23", "8", "5", "0", "ZW"] as const).flatMap((rate, index) => {
        const group = lines.filter(line => line.vatRate === rate);
        if (!group.length) return [];
        const suffix = ["1", "2", "3", "6_1", "7"][index];
        return [
            [`P_13_${suffix}`, round(group.reduce((sum, line) => sum + line.net, 0))],
            ...(index < 3 ? [[`P_14_${suffix}`, round(group.reduce((sum, line) => sum + line.vat, 0))]] : [])
        ];
    }));

    setText(fa, {
        KodWaluty: "PLN",
        P_1: issueDate,
        P_1M: issuePlace,
        P_2: input.invoiceNumber ?? `eFA/${year}/${String(month).padStart(2, "0")}/1`,
        P_6: input.deliveryDate ?? postingDate,
        P_15: round(lines.reduce((sum, line) => sum + line.gross, 0)),
        RodzajFaktury: "VAT"
    });
    if (!issuePlace) fa.splice(fa.findIndex(node => "P_1M" in node), 1);
    fa.splice(fa.findIndex(node => "P_13_1" in node), 2, ...elements(totals));

    // Optional markings
    const markingMpp = input.markingMpp ?? false;
    const markingMk = input.markingMk ?? false;
    const markingFp = input.markingFp ?? false;
    const markingTp = input.markingTp ?? false;

    // Cash accounting
    setText(annotations, { P_16: markingMk ? 1 : 2 });
    // Self-billing
    setText(annotations, { P_17: 2 });
    // Reverse charge
    setText(annotations, { P_18: 2 });
    // Mandatory split payment
    setText(annotations, { P_18A: markingMpp ? 1 : 2 });

    // VAT exemption
    setText(children(annotations, "Zwolnienie"), { P_19N: 1 });
    // New means of transport
    setText(children(annotations, "NoweSrodkiTransportu"), { P_22N: 1 });
    // Triangular transaction
    setText(annotations, { P_23: 2 });
    // Margin scheme
    setText(children(annotations, "PMarzy"), { P_PMarzyN: 1 });

    // Invoice positions
    const invoiceLines = lines.map((line, index) => ({ FaWiersz: elements({
        NrWierszaFa: index + 1,
        P_7: line.name,
        P_8A: line.unit,
        P_8B: line.quantity,
        ...(input.priceType === "gross" ? { P_9B: line.unitPrice, P_11A: line.gross } : { P_9A: line.unitPrice, P_11: line.net }),
        P_11Vat: line.vat,
        P_12: line.vatRate === "ZW" ? "zw" : line.vatRate === "0" ? "0 KR" : line.vatRate,
        ...(line.gtu && { GTU: line.gtu }), ...(line.procedure && { Procedura: line.procedure })
    }) }));
    const additionalInformation = lines.flatMap((line, index) => line.additionalInformation ? [{ DodatkowyOpis: elements({
        NrWiersza: index + 1, Klucz: "Additional information", Wartosc: line.additionalInformation
    }) }] : []);
    fa.splice(fa.findIndex(node => node["#text"] === "{{INVOICE_LINES}}"), 1,
        ...elements({ ...(markingFp && { FP: 1 }), ...(markingTp && { TP: 1 }) }),
        ...additionalInformation, ...invoiceLines);

    // Payment
    const deadline = new Date(`${postingDate}T00:00:00Z`);
    deadline.setUTCDate(deadline.getUTCDate() + (input.paymentDays ?? 7));
    const paymentTerm = children(payment, "TerminPlatnosci");
    if (input.paymentTerm) {
        paymentTerm.splice(paymentTerm.findIndex(node => "Termin" in node), 1, { TerminOpis: elements({
            Ilosc: input.paymentTerm.periodLength, Jednostka: input.paymentTerm.periodUnit, ZdarzeniePoczatkowe: input.paymentTerm.startingEvent
        }) });
    } else {
        setText(paymentTerm, { Termin: input.paymentDeadline ?? date(deadline) });
    }
    setText(payment, { FormaPlatnosci: input.paymentType ?? "6" });
    const bankAccount = input.bankAccount ?? appUser.bankAccountNumber;
    payment.splice(payment.findIndex(node => node["#text"] === "{{BANK_ACCOUNT}}"), 1,
        ...(bankAccount ? [{ RachunekBankowy: elements({ NrRB: bankAccount }) }] : []));
    payment.push(...elements({ ...(input.paymentLink && { LinkDoPlatnosci: input.paymentLink }), ...(input.ksefPaymentId && { IPKSeF: input.ksefPaymentId }) }));

    // Footer
    if (input.footers?.length)
        root.push({ Stopka: input.footers.map(footer => ({ Informacje: elements({ StopkaFaktury: footer }) })) });

    // Formatting with spacing 2
    return xmlBuilder.build(document);
}

function setText(parent: any[], values: Record<string, string | number>): void {
    for (const [name, value] of Object.entries(values)) {
        children(parent, name).splice(0, Infinity, { "#text": value });
    }
}