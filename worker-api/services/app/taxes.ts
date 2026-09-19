import {Repository} from "../../repository/d1";
import {AppTaxRecordDb} from "../../types/taxes";

export class TaxesService {
    constructor(private readonly repo: Repository) {}

    async get(filters: Record<string, any> = {}, ownerId?: string, isSuperAdmin = false): Promise<any[]> {
        const scopedFilters = isSuperAdmin || !ownerId ? filters : { ...filters, ownerId };
        const rows = await this.repo.getAll<AppTaxRecordDb>("taxes", scopedFilters);
        return rows.map(row => ({ ...row, purchasesSummary: JSON.parse(row.purchasesSummary) }));
    }

    async delete(filters: Record<string, any> = {}, ownerId?: string, isSuperAdmin = false): Promise<{ success: boolean; changes: number; filters: Record<string, any>; status: number }> {
        const scopedFilters = isSuperAdmin || !ownerId ? filters : { ...filters, ownerId };
        const result = await this.repo.delete("taxes", scopedFilters);
        return { success: result.success, changes: result.changes, filters: scopedFilters, status: result.changes === 0 ? 404 : 200 };
    }
}