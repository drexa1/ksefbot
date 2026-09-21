import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {corsHeaders, getAuthUser} from "../../auth";
import {AppUser, AppUserUpdate} from "../../types/users";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function get(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const filters: Record<string, any> = {};
    for (const [key, value] of url.searchParams.entries()) {
        filters[key] = value;
    }
    // 🐣 Allow checking if a user exists by phone without assuming any users yet
    const isNewLookup = !!filters.phone;
    const appUser = isNewLookup ? undefined : await getAuthUser(req, env);
    // Allow to fetch users only to superadmin
    if (!isNewLookup && appUser!.tier !== 0)
        return new Response("Unauthorized", { status: 401, headers: corsHeaders });
    const rows = await getRepo(env).getAll<AppUser>("users", filters);
    return rows.length === 0
        ? Response.json({ success: false, error: "No user found", filters: filters }, { status: 404 })
        : Response.json(rows, { status: 200 });
}

/**
 * 🐣 User creation triggered by the onboarding flow.
 * 👀 At this point there is no user to verify yet
 */
export async function post(req: Request, env: Env): Promise<Response> {
    // Never allow client to control tier, apiKey, or creation/update timestamps
    const payload = await req.json() as AppUser;
    const { tier, apiKey, createdAt, updatedAt, ...payloadData } = payload;
    const record = { ...payloadData, tier: 1, updatedAt: new Date().toISOString() };
    try {
        await getRepo(env).save<AppUser>("users", record);
        return Response.json({ success: true, id: record.id }, { status: 201 });
    } catch (error) {
        if (String(error).includes("UNIQUE constraint failed"))
            return Response.json({ success: false, error: "User already exists", id: record.id }, { status: 409 });
        throw error;
    }
}

export async function put(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    // Allow to update users only to superadmin
    if (appUser.tier !== 0)
        return new Response("Unauthorized", { status: 401, headers: corsHeaders });
    const payload = await req.json() as AppUser;
    // Never allow client to change id, tier, apiKey or creation/update timestamp
    const { id, tier, apiKey, createdAt, updatedAt, ...updatePayload } = payload;
    const result = await getRepo(env).update<AppUserUpdate>("users", {
        ...updatePayload, tier: 1, updatedAt: new Date().toISOString()
    }, { id: id });
    return result.changes === 0
        ? Response.json({ success: false, error: "No user found", id: id }, { status: 404 })
        : Response.json({ success: true, changes: result.changes, id: id }, { status: result.success ? 200 : 400 });
}

export async function del(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    // Only superadmins can delete users
    if (appUser.tier !== 0)
        return new Response("Unauthorized", {status: 401, headers: corsHeaders });
    const url = new URL(req.url);
    const filters: Record<string, any> = {};
    for (const [key, value] of url.searchParams.entries()) {
        filters[key] = value;
    }
    const result = await getRepo(env).delete("users", filters);
    return result.changes === 0
        ? Response.json({ success: false, error: "No user found", filters }, { status: 404 })
        : Response.json({ success: result.success, changes: result.changes, ...filters }, { status: 200 });
}
