import {ContractorUI} from "../api/contractors";

export async function generateInvoiceXml(userContractor: ContractorUI, form: HTMLFormElement): Promise<string> {

    const response = await fetch("/schemas/invoice-template.xml");
    const templateXml = await response.text();
    const xmlDocument = new DOMParser().parseFromString(templateXml, "application/xml");
    const root = xmlDocument.documentElement;

    const fa = root.querySelector("Fa")!;
    const seller = root.querySelector("Podmiot1")!;
    const buyer = root.querySelector("Podmiot2")!;
    const annotations = fa.querySelector("Adnotacje")!;
    const payment = fa.querySelector("Platnosc")!;

    // Header
    root.querySelector("KodFormularza")!.textContent = "FA";
    root.querySelector("WariantFormularza")!.textContent = "3";
    root.querySelector("SystemInfo")!.textContent = "KSeF Bot";
    root.querySelector("DataWytworzeniaFa")!.textContent = new Date().toISOString();

    // Seller
    const sellerAddress = `${userContractor.town}, ${userContractor.postalCode}, ${userContractor.building}/${userContractor.apartment}`;
    seller.querySelector("NIP")!.textContent = userContractor.nip;
    seller.querySelector("Nazwa")!.textContent = `${userContractor.name}`;
    seller.querySelector("AdresL1")!.textContent = sellerAddress;
    seller.querySelector("KodKraju")!.textContent = "PL";

    // Customer
    const customerName = form.querySelector<HTMLInputElement>("#contractorName")?.value.trim() ?? "";
    const customerNip = form.querySelector<HTMLInputElement>("#contractorNipInput")?.value.trim() ?? "";
    const customerTown = form.querySelector<HTMLInputElement>("#contractorTown")?.value.trim() ?? "";
    const customerPostalCode = form.querySelector<HTMLInputElement>("#contractorPostalCode")?.value.trim() ?? "";
    const customerStreet = form.querySelector<HTMLInputElement>("#contractorStreet")?.value.trim() ?? "";
    const customerBuilding = form.querySelector<HTMLInputElement>("#contractorBuilding")?.value.trim() ?? "";
    const customerApartment = form.querySelector<HTMLInputElement>("#contractorApartment")?.value.trim() ?? "";
    const customerAddress = `${customerTown}, ${customerPostalCode}, ${customerStreet} ${customerBuilding}${customerApartment ? `/${customerApartment}` : ""}`;

    buyer.querySelector("NIP")!.textContent = customerNip;
    buyer.querySelector("Nazwa")!.textContent = customerName;
    buyer.querySelector("AdresL1")!.textContent = customerAddress;
    buyer.querySelector("KodKraju")!.textContent = "PL";

    // Additional entity
    const additionalEntity = form.querySelector<HTMLInputElement>('input[name="additionalEntity"]:checked')?.value ?? "";
    if (additionalEntity === "jst") {
        buyer.querySelector("JST")!.textContent = "1";
        buyer.querySelector("GV")!.textContent = "2";
    } else if (additionalEntity === "gv") {
        buyer.querySelector("JST")!.textContent = "2";
        buyer.querySelector("GV")!.textContent = "1";
    } else {
        buyer.querySelector("JST")!.textContent = "2";
        buyer.querySelector("GV")!.textContent = "2";
    }

    // Invoice
    fa.querySelector("KodWaluty")!.textContent = "PLN";
    fa.querySelector("P_1")!.textContent = form.querySelector<HTMLInputElement>("#issueDate")?.value.trim() ?? "";
    fa.querySelector("P_1M")!.textContent = form.querySelector<HTMLInputElement>("#issuePlace")?.value.trim() ?? "";
    fa.querySelector("P_2")!.textContent = form.querySelector<HTMLInputElement>("#invoiceNumber")?.value.trim() ?? "";
    fa.querySelector("P_6")!.textContent = form.querySelector<HTMLInputElement>("#deliveryDate")?.value.trim() ?? "";
    fa.querySelector("P_13_1")!.textContent = String(parseFloat(form.querySelector<HTMLElement>("#totalNet")?.textContent ?? "0") || 0);
    fa.querySelector("P_14_1")!.textContent = String(parseFloat(form.querySelector<HTMLElement>("#totalVAT")?.textContent ?? "0") || 0);
    fa.querySelector("P_15")!.textContent = String(parseFloat(form.querySelector<HTMLElement>("#totalGross")?.textContent ?? "0") || 0);
    fa.querySelector("RodzajFaktury")!.textContent = "VAT";

    // Optional markings
    const markingMpp = form.querySelector<HTMLInputElement>("#markingMpp")?.checked ?? false;
    const markingMk = form.querySelector<HTMLInputElement>("#markingMk")?.checked ?? false;
    const markingFp = form.querySelector<HTMLInputElement>("#markingFp")?.checked ?? false;
    const markingTp = form.querySelector<HTMLInputElement>("#markingTp")?.checked ?? false;

    // Cash accounting
    annotations.querySelector("P_16")!.textContent = markingMk ? "1" : "2";
    // Self-billing
    annotations.querySelector("P_17")!.textContent = markingFp ? "1" : "2";
    // Reverse charge
    annotations.querySelector("P_18")!.textContent = markingTp ? "1" : "2";
    // Mandatory split payment
    annotations.querySelector("P_18A")!.textContent = markingMpp ? "1" : "2";

    // VAT exemption
    annotations.querySelector("P_19N")!.textContent = "1";
    // New means of transport
    annotations.querySelector("P_22N")!.textContent = "1";
    // Triangular transaction
    annotations.querySelector("P_23")!.textContent = "2";
    // Margin scheme
    annotations.querySelector("P_PMarzyN")!.textContent = "1";

    // Invoice positions
    const invoiceLinesPlaceholder = Array.from(fa.childNodes).find(node => node.textContent?.includes("{{INVOICE_LINES}}"))!;
    const rows = Array.from(form.querySelectorAll(".item-row")) as HTMLTableRowElement[];
    rows.forEach((row, index) => {
        const number = index + 1;
        const invoiceRow = xmlDocument.createElementNS(root.namespaceURI, "FaWiersz");

        const description = row.querySelector<HTMLInputElement>('input[id^="itemName"]')?.value.trim() ?? "";
        const unit = row.querySelector<HTMLInputElement>('input[id^="itemUnit"]')?.value.trim() ?? "";
        const quantity = parseFloat(row.querySelector<HTMLInputElement>('input[id^="itemQuantity"]')?.value ?? "0") || 0;
        const unitPrice = parseFloat(row.querySelector<HTMLInputElement>('input[id^="itemPrice"]')?.value ?? "0") || 0;
        const net = parseFloat(row.querySelector<HTMLInputElement>('input[id^="itemNet"]')?.value ?? "0") || 0;
        const vat = parseFloat(row.querySelector<HTMLInputElement>('input[id^="itemVATamount"]')?.value ?? "0") || 0;
        const vatRate = (row.querySelector('select[id^="itemVAT"]') as unknown as HTMLSelectElement)?.value ?? "23";

        Object.entries({
            NrWierszaFa: String(number),
            P_7: description,
            P_8A: unit,
            P_8B: String(quantity),
            P_9A: String(unitPrice),
            P_11: String(net),
            P_11Vat: String(vat),
            P_12: vatRate === "ZW" ? "0" : vatRate
        }).forEach(([name, value]) => {
            invoiceRow.appendChild(xmlDocument.createTextNode("\n" + " ".repeat(12)));
            const element = xmlDocument.createElementNS(root.namespaceURI, name);
            element.textContent = value;
            invoiceRow.appendChild(element);
        });
        invoiceRow.appendChild(xmlDocument.createTextNode("\n" + " ".repeat(8)));
        fa.insertBefore(xmlDocument.createTextNode("\n" + " ".repeat(8)), invoiceLinesPlaceholder);
        fa.insertBefore(invoiceRow, invoiceLinesPlaceholder);
    });
    invoiceLinesPlaceholder.remove();
    payment.parentNode!.insertBefore(xmlDocument.createTextNode("\n" + " ".repeat(8)), payment);

    // Payment
    payment.querySelector("Termin")!.textContent = form.querySelector<HTMLInputElement>("#paymentDeadline")?.value.trim() ?? "";
    payment.querySelector("FormaPlatnosci")!.textContent = (form.querySelector("#paymentType") as unknown as HTMLSelectElement)?.value ?? "";
    const bankAccountPlaceholder = Array.from(payment.childNodes).find(node => node.textContent?.includes("{{BANK_ACCOUNT}}"))!;
    const bankAccount = form.querySelector<HTMLInputElement>("#bankAccount")?.value.trim() ?? "";
    if (bankAccount) {
        const bankAccountElement = xmlDocument.createElementNS(root.namespaceURI, "RachunekBankowy");
        const nrRb = xmlDocument.createElementNS(root.namespaceURI, "NrRB");
        nrRb.textContent = bankAccount;
        bankAccountElement.appendChild(xmlDocument.createTextNode("\n" + " ".repeat(16)));
        bankAccountElement.appendChild(nrRb);
        bankAccountElement.appendChild(xmlDocument.createTextNode("\n" + " ".repeat(12)));
        payment.insertBefore(xmlDocument.createTextNode("\n" + " ".repeat(12)), bankAccountPlaceholder);
        payment.insertBefore(bankAccountElement, bankAccountPlaceholder);
        payment.insertBefore(xmlDocument.createTextNode("\n" + " ".repeat(8)), bankAccountPlaceholder);
    }
    bankAccountPlaceholder.remove();

    // Formatting with spacing 2
    return new XMLSerializer().serializeToString(xmlDocument)
        .replace(/^ +/gm, spaces => ' '.repeat(spaces.length / 2))
        .replace('<?xml version="1.0" encoding="utf-8"?>', '<?xml version="1.0" encoding="utf-8"?>\n');
}