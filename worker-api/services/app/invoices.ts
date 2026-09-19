import {Repository} from "../../repository/d1";
import {AppInvoice} from "../../types/invoices";

export class InvoicesService {
    constructor(private readonly repo: Repository) {}

    async get(filters: Record<string, any> = {}, ownerId?: string, isSuperAdmin = false): Promise<any[]> {
        const scopedFilters = isSuperAdmin || !ownerId ? filters : { ...filters, ownerId };
        const rows = await this.repo.getAll<AppInvoice>("invoices", scopedFilters);
        if (!rows.length) return [];
        return rows.map(row => JSON.parse(row.jsonData));
    }

    async delete(filters: Record<string, any> = {}, ownerId?: string, isSuperAdmin = false): Promise<{ success: boolean; changes: number; filters: Record<string, any>; status: number }> {
        const scopedFilters = isSuperAdmin || !ownerId ? filters : { ...filters, ownerId };
        const result = await this.repo.delete("invoices", scopedFilters);
        return { success: result.success, changes: result.changes, filters: scopedFilters, status: result.changes === 0 ? 404 : 200 };
    }
}