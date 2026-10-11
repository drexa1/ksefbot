import {SubmissionStatus} from "../../worker-api/types/ksef";

export async function submitInvoice(xmlContent: string, notes: string): Promise<{ sessionReferenceNumber: string, invoiceReferenceNumber: string }> {
    const url = `${import.meta.env.VITE_WORKER_URL}/app/invoices/ingest`;
    const formData = new FormData();
    formData.append("file", new File([xmlContent], "invoice.xml", { type: "application/xml" }));
    formData.append("type", "sales");
    formData.append("notes", JSON.stringify(notes));
    const response = await fetch(url, {
        method: "POST",
        headers: { "X-API-Key": import.meta.env.VITE_API_KEY },
        body: formData
    });
    if (!response.ok)
        throw new Error(`Failed to submit invoice: ${response.status}`);
    return await response.json();
}

export async function downloadReceipt(sessionRef: string, invoiceRef: string): Promise<SubmissionStatus> {
    const url = new URL(`${import.meta.env.VITE_WORKER_URL}/ksef/sales/receipt`);
    url.searchParams.set("sessionReferenceNumber", sessionRef);
    url.searchParams.set("invoiceReferenceNumber", invoiceRef);
    const response = await fetch(url, {
        method: "GET",
        headers: { "X-API-Key": import.meta.env.VITE_API_KEY, "Accept": "application/json" }
    });
    if (!response.ok)
        throw new Error(`Failed to download submission receipt: ${response.status}`);
    return await response.json();
}