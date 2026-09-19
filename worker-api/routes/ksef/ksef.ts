import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {service} from "../../services/services";

export async function fetchKsefInvoices(env: Env, appUser: AppUser, subjectType: "Subject1" | "Subject2", from: Date, to: Date) {
    return await service.ksef.fetchInvoices(env, appUser, subjectType, from, to);
}

export async function getInvoicesFor(req: Request, env: Env, subjectType: "Subject1" | "Subject2"): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const fromParam = url.searchParams.get("from")!;
    const toParam = url.searchParams.get("to")!;
    const from = new Date(fromParam);
    const to = new Date(toParam);
    if (isNaN(from.getTime()) || isNaN(to.getTime()) || from > to)
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    if ((to.getTime() - from.getTime()) / 86_400_000 > 90)
        return Response.json({ success: false, error: `The maximum date range supported by KSeF is 3 calendar months.` }, { status: 400 });
    try {
        const result = await fetchKsefInvoices(env, appUser, subjectType, from, to);
        return Response.json({
            success: result.length > 0,
            result,
            ...(result.length === 0 && { error: "No invoices found for the specified date range." })
        }, { status: 200 });
    } catch (error: any) {
        if (String(error).includes("Too Many Requests"))
            return Response.json({ success: false, error: "The limit of 20 requests per hour has been exceeded." }, { status: 429 });
        throw error;
    }
}