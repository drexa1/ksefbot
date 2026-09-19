import {Env} from "../../worker";
import {KsefClient} from "../../clients/ksef";
import {Repository} from "../../repository/d1";
import {AppUser} from "../../types/users";
import {AppInvoice} from "../../types/invoices";

export class KsefService {
    constructor(private readonly repo: Repository) {}

    async fetchInvoices(env: Env, appUser: AppUser, subjectType: "Subject1" | "Subject2", from: Date, to: Date) {
        const ksefClient = new KsefClient(env);
        const invoices = await ksefClient.queryPurchaseInvoices(env, appUser, subjectType, from, to);
        await this.saveInvoices(invoices);
        return invoices.map(row => JSON.parse(row.jsonData));  // return the JSON formatted
    }

    /// Cache in app
    private async saveInvoices(invoices: Awaited<AppInvoice & { ownerId: string }>[]) {
        const saved = [];
        const existing = [];
        for (const invoice of invoices) {
            try {
                await this.repo.save<AppInvoice>("invoices", invoice);
                saved.push(invoice);
            } catch (error) {
                if (String(error).includes("UNIQUE constraint failed")) {
                    console.warn("Invoice already existed in the app:", invoice.id);
                    existing.push(invoice.id);
                } else {
                    throw error;
                }
            }
        }
    }
}