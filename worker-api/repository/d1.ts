import { D1Database } from "@cloudflare/workers-types";

type DBResult = { success: boolean, changes: number, lastInsertId?: number | bigint };

export interface DBDriver {
    get(sql: string, values: any[]): Promise<any>;
    getAll(sql: string, values?: any[]): Promise<any[]>;
    save(sql: string, values: any[]): Promise<DBResult>;
    update(sql: string, values: any[]): Promise<DBResult>;
    delete(sql: string, values: any[]): Promise<DBResult>;
}

export class D1Driver implements DBDriver {
    constructor(private db: D1Database) {}

    async get(sql: string, values: any[]) {
        return await this.db.prepare(sql).bind(...values).first();
    }

    async getAll(sql: string, values: any[] = []) {
        const stmt = this.db.prepare(sql);
        const { results } = values ? await stmt.bind(...values).all() : await stmt.all();
        return results;
    }

    async save(sql: string, values: any[]): Promise<DBResult> {
        const result = await this.db.prepare(sql).bind(...values).run();
        return { success: result.success, changes: result.meta.changes, lastInsertId: result.meta.last_row_id };
    }

    async update(sql: string, values: any[]): Promise<DBResult> {
        const result = await this.db.prepare(sql).bind(...values).run();
        return { success: result.success, changes: result.meta.changes, lastInsertId: result.meta.last_row_id };
    }

    async delete(sql: string, values: any[]): Promise<DBResult> {
        const result = await this.db.prepare(sql).bind(...values).run();
        return { success: result.success, changes: result.meta.changes, lastInsertId: result.meta.last_row_id };
    }
}

export class Repository {
    constructor(private driver: DBDriver) {}

    async get<T>(table: string, filters: Record<string, any>): Promise<T> {
        const conditions = Object.keys(filters).map(field => `"${field}" = ?`).join(" AND ");
        const values = Object.values(filters);
        const sql = `SELECT * FROM ${table} WHERE ${conditions} LIMIT 1`;
        return await this.driver.get(sql, values) as T;
    }

    async getAll<T>(
        table: string,
        filters?: Record<string, any>,
        range?: { field: string; start?: string; startInclusive?: boolean; end?: string; endInclusive?: boolean },
        cols?: string[]
    ): Promise<T[]> {
        const select = cols?.length ? cols.map(c => `"${c}"`).join(", ") : "*";
        const conditions: string[] = [];
        const values: any[] = [];
        for (const [field, value] of Object.entries(filters ?? {})) {
            conditions.push(`"${field}" = ?`);
            values.push(value);
        }
        if (range?.start) {
            conditions.push(`"${range.field}" ${!range.startInclusive ? ">" : ">="} ?`);
            values.push(range.start);
        }
        if (range?.end) {
            conditions.push(`"${range.field}" ${!range.endInclusive ? "<" : "<="} ?`);
            values.push(range.end);
        }
        const where = conditions.length ? ` WHERE ${conditions.join(" AND ")}` : "";
        const sql = `SELECT ${select} FROM ${table} ${where}`;
        return await this.driver.getAll(sql, values) as T[];
    }

    async save<T>(table: string, data: T, ignore = false): Promise<DBResult> {
        const { sql, values } = this.buildInsert(table, data, ignore);
        return this.driver.save(sql, values);
    }

    private buildInsert<T>(table: string, data: T, ignore = false) {
        const entries = Object.entries(data as any);
        const columns = entries.map(([k]) => `"${k}"`).join(", ");
        const placeholders = entries.map(() => "?").join(", ");
        const values = entries.map(([_, v]) => v instanceof Date ? v.toISOString() : v);
        const sql = `INSERT ${ignore ? "OR IGNORE " : ""}INTO ${table} (${columns}) VALUES (${placeholders})`;
        return { sql, values };
    }

    async update<T>(table: string, data: T, filters?: Record<string, any>): Promise<DBResult> {
        const entries = Object.entries(data as any);
        if (!entries.length) return { success: false, changes: 0 };
        const assignments = entries.map(([k]) => `"${k}" = ?`).join(", ");
        const values = entries.map(([_, v]) => v instanceof Date ? v.toISOString() : v);
        const conditions = Object.keys(filters ?? {}).map(k => `"${k}" = ?`).join(" AND ");
        const filterValues = Object.values(filters ?? {});
        const sql = `UPDATE ${table} SET ${assignments} ${conditions ? ` WHERE ${conditions}` : ""}`;
        return this.driver.update(sql, [...values, ...filterValues]);
    }

    async delete(table: string, filters: Record<string, any>): Promise<DBResult> {
        const entries = Object.entries(filters);
        const where = entries.length > 0 ? ` WHERE ${entries.map(([field]) => `"${field}" = ?`).join(" AND ")}` : "";
        const values = entries.map(([, value]) => value);
        const sql = `DELETE FROM ${table}${where}`;
        return this.driver.delete(sql, values);
    }
}
