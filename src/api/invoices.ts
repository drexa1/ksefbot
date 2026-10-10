import {InvoiceInput} from "../../worker-api/types/invoices";

export async function generateInvoice(input: InvoiceInput): Promise<string> {
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
