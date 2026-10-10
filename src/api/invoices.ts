import {InvoiceInput} from "../../worker-api/types/invoices";

export async function invoiceFromForm(form: HTMLFormElement, customerId: string): Promise<string> {
    const value = (selector: string, root: Element = form): string => root.querySelector<HTMLInputElement>(selector)!.value.trim();
    const checked = (selector: string): boolean => form.querySelector<HTMLInputElement>(selector)!.checked;

    return generateInvoice({
        // Invoice data
        invoiceNumber: value("#invoiceNumber"),
        issueDate: value("#issueDate"),
        issuePlace: value("#issuePlace"),
        postingDate: value("#postingDate"),
        deliveryDate: value("#deliveryDate"),
        // Optional markings
        markingMpp: checked("#markingMpp"),
        markingMk: checked("#markingMk"),
        markingFp: checked("#markingFp"),
        markingTp: checked("#markingTp"),
        // Customer section
        customerId: customerId,
        customerEmail: value("#contractorMail"),
        additionalEntity: value('input[name="additionalEntity"]:checked') as InvoiceInput["additionalEntity"],
        // Items section
        priceType: value('input[name="priceType"]:checked') as InvoiceInput["priceType"],
        items: Array.from(form.querySelectorAll(".item-row")).map(row => ({
            name: value('input[id^="itemName"]', row),
            unit: value('input[id^="itemUnit"]', row),
            quantity: Number(value('input[id^="itemQuantity"]', row)),
            unitPrice: Number(value('input[id^="itemPrice"]', row)),
            vatRate: value('select[id^="itemVAT"]', row) as NonNullable<InvoiceInput["items"]>[number]["vatRate"],
            gtu: checked("#additionalColumnGTU") ? value('input[id^="itemGTU"]', row) : undefined,
            procedure: checked("#procedureSymbols") ? value('input[id^="itemProcedureSymbols"]', row) : undefined,
            additionalInformation: checked("#additionalInformation") ? value('input[id^="itemAdditionalInformation"]', row) : undefined
        })),
        // Payment section
        paymentType: value("#paymentType") as InvoiceInput["paymentType"],
        bankAccount: value("#bankAccount"),
        invoicePaid: checked("#invoicePaid"),
        ...(checked("#paymentTermDescription") ? {
            paymentTerm: {
                periodLength: Number(value("#periodLength")),
                periodUnit: value("#periodUnit"),
                startingEvent: value("#startingEvent")
            }
        } : {
            paymentDays: Number(value("#paymentDays")),
            paymentDeadline: value("#paymentDeadline")
        }),
        paymentLink: value("#paymentLink"),
        ksefPaymentId: value("#ksefPaymentId"),
        // Footers section
        footers: Array.from(form.querySelectorAll<HTMLTextAreaElement>("#footersContainer textarea"))
            .map(footer => footer.value.trim())
            .filter(Boolean)
    });
}

async function generateInvoice(input: InvoiceInput): Promise<string> {
    const response = await fetch(`${import.meta.env.VITE_WORKER_URL}/app/invoices/generate`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "Accept": "application/xml",
            "X-API-Key": import.meta.env.VITE_API_KEY
        },
        body: JSON.stringify(input)
    });
    if (!response.ok)
        throw new Error((await response.json() as { error: string }).error);
    return await response.text();
}
