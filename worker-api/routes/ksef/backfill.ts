import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {InvoicesBackfill} from "../../types/invoices";
import {D1Driver, Repository} from "../../repository/d1";
import {findUncoveredPeriods} from "./sync-periods";

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
    return Response.json({ success: true, ...job, progress: await estimateProgress(env, job) });
}

async function estimateProgress(env: Env, backfill: InvoicesBackfill): Promise<number> {
    if (backfill.status === "completed") return 1;
    if (backfill.status === "queued") return 0;
    const to = new Date(`${backfill.createdAt.slice(0, 10)}T00:00:00Z`);
    const from = new Date(Date.UTC(to.getUTCFullYear() - 1, 0, 1));
    const uncovered = await findUncoveredPeriods(env, backfill.ownerId, "sales", from, to);
    const remaining = uncovered.reduce((days, period) => days + (Date.parse(period.to) - Date.parse(period.from)) / 86_400_000 + 1, 0);
    const total = (to.getTime() - from.getTime()) / 86_400_000 + 1;
    return Math.min(0.99, 1 - remaining / total);
}

async function refreshJob(env: Env, backfill: InvoicesBackfill): Promise<InvoicesBackfill> {
    if (backfill.status !== "queued" && backfill.status !== "running" && backfill.status !== "throttled")
        return backfill;
    const workflow = await env.BACKFILL_JOB.get(backfill.ownerId).then(workflow => workflow.status());
    if (workflow.status !== "errored" && workflow.status !== "terminated" && workflow.status !== "complete")
        return backfill;
    const status = workflow.status === "complete" ? "completed" : backfill.status === "throttled" ? "throttled" : "failed";
    await getRepo(env).update("invoices_backfill", { status, error: workflow.error?.message, updatedAt: new Date().toISOString() }, { ownerId: backfill.ownerId });
    return { ...backfill, status: status, error: workflow.error?.message };
}
