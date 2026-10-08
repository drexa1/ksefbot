import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {InvoicesBackfill} from "../../types/invoices";
import {D1Driver, Repository} from "../../repository/d1";

let repo: Repository;
const getRepo = (env: Env) => repo ??= new Repository(new D1Driver(env.D1));

export async function start(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const backfillStatus = await getRepo(env).get<InvoicesBackfill>("invoices_backfill", { ownerId: appUser.id });
    if (backfillStatus)
        return Response.json({ success: false, existing: true }, { status: 409 });
    const createdBackfillStatus = await getRepo(env).save("invoices_backfill", { ownerId: appUser.id, status: "queued" }, true);
    if (!createdBackfillStatus.changes)
        return Response.json({ success: false, existing: true }, { status: 409 });
    try {
        await env.BACKFILL_JOB.create({ id: appUser.id, params: { userId: appUser.id } });
    } catch (error) {
        await getRepo(env).update("invoices_backfill", { status: "failed", error: String(error), updatedAt: new Date().toISOString()}, { ownerId: appUser.id });
        return Response.json({ success: false, error: String(error) }, { status: 502 });
    }
    return Response.json({ success: true, existing: false }, { status: 202 });
}

export async function status(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const invoicesBackfill = await getRepo(env).get<InvoicesBackfill>("invoices_backfill", {ownerId: appUser.id});
    if (!invoicesBackfill)
        return Response.json({ success: false, error: "No Backfill job found" }, { status: 404 });
    const job = await refreshJob(env, invoicesBackfill);
    return Response.json({ success: true, ...job });
}

async function refreshJob(env: Env, backfill: InvoicesBackfill) {
    if (backfill.status !== "queued" && backfill.status !== "running" && backfill.status !== "throttled")
        return backfill;
    const workflow = await env.BACKFILL_JOB.get(backfill.ownerId).then(workflow => workflow.status());
    if (workflow.status !== "errored" && workflow.status !== "terminated" && workflow.status !== "complete")
        return backfill;
    const status = workflow.status === "complete" ? "completed" : backfill.status === "throttled" ? "throttled" : "failed";
    await getRepo(env).update("invoices_backfill", { status, error: workflow.error?.message, updatedAt: new Date().toISOString() }, { ownerId: backfill.ownerId });
    return { ...backfill, status: status, error: workflow.error?.message };
}
