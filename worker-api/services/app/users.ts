import {Repository} from "../../repository/d1";
import {AppUser, AppUserUpdate} from "../../types/users";

export class UsersService {
    constructor(private readonly repo: Repository) {}

    async get(filters: Record<string, any>): Promise<AppUser[]> {
        return this.repo.getAll<AppUser>("users", filters);
    }

    async create(payload: AppUser): Promise<{success: true, id: string}> {
        const {tier, apiKey, createdAt, updatedAt, ...payloadData} = payload;
        const record = {...payloadData, tier: 1, updatedAt: new Date().toISOString()};
        await this.repo.save<AppUser>("users", record);
        return {success: true, id: record.id};
    }

    async update(payload: AppUser): Promise<{success: boolean, changes: number, id: string}> {
        const {id, tier, apiKey, createdAt, updatedAt, ...updatePayload} = payload;
        const result = await this.repo.update<AppUserUpdate>("users", {
            ...updatePayload,
            tier: 1,
            updatedAt: new Date().toISOString()
        }, {id});
        return {success: result.success, changes: result.changes, id};
    }

    async delete(filters: Record<string, any> = {}): Promise<{success: boolean; changes: number; filters: Record<string, any>}> {
        const result = await this.repo.delete("users", filters);
        return {success: result.success, changes: result.changes, filters};
    }
}