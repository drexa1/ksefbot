import {Repository} from "../../repository/d1";
import {AppContractor, AppContractorUpdate} from "../../types/contractors";

export class ContractorsService {
    constructor(private readonly repo: Repository) {}

    async get(filters: Record<string, any>): Promise<AppContractor[]> {
        return await this.repo.getAll<AppContractor>("contractors", filters);
    }

    async create(payload: AppContractor, ownerId: string): Promise<{ success: true; id: string; status: number }> {
        // Never allow client to control id, ownership, or creation/update timestamps
        const { id, createdAt, updatedAt, ...payloadData } = payload;
        const record = { ...payloadData, id: crypto.randomUUID(), ownerId, updatedAt: new Date().toISOString() };
        await this.repo.save<AppContractor>("contractors", record);
        return { success: true, id: record.id, status: 201 };
    }

    async update(payload: AppContractor & { ownerId?: string }, ownerId: string): Promise<{ success: boolean; changes: number; id: string; status: number }> {
        // Never allow client to change id, ownership, or creation/update timestamp
        const { id, ownerId: payloadOwnerId, createdAt, updatedAt, ...updatePayload } = payload;
        const result = await this.repo.update<AppContractorUpdate>("contractors", {
            ...updatePayload,
            updatedAt: new Date().toISOString(),
        }, { id, ownerId });
        return { success: result.success, changes: result.changes, id, status: result.changes === 0 ? 404 : 200 };
    }

    async delete(filters: Record<string, any>): Promise<{ success: boolean; changes: number; filters: Record<string, any>; status: number }> {
        const result = await this.repo.delete("contractors", filters);
        return { success: result.success, changes: result.changes, filters, status: result.changes === 0 ? 404 : 200 };
    }
}