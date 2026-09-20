abstract class DB {
    abstract get(id: string): Promise<any>;
    abstract getAll(prefix: string): Promise<any>;
    abstract save(id: string, data: any): Promise<string>;
    abstract update(id: string, data: any): Promise<void>;
    // noinspection JSUnusedGlobalSymbols
    delete(_: string): never { throw new Error("Unsupported"); }
}

export class CloudflareKV extends DB {
    private KV: any;

    // @with
    binding(kvBinding: any): this {
        this.KV = kvBinding;
        return this;
    }


    async get(id: string): Promise<any> {
        return await this.KV.get(id);
    }

    async getAll(prefix?: string): Promise<any> {
        const list = await this.KV.list({ prefix });
        return await Promise.all(
            list.keys.map(async (k: any) => {
                const data = await this.KV.get(k.name);
                return { id: k.name, ...JSON.parse(data!) };
            })
        );
    }

    async save(id: string, data: any): Promise<string> {
        await this.KV.put(id, JSON.stringify(data));
        return id;
    }

    async update(id: string, data: any): Promise<void> {
        await this.KV.put(id, JSON.stringify(data));
    }
}