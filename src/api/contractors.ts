/// <reference types="vite/client"/>
import {AppContractor} from "../../worker-api/types/contractors";

/// UI model
export interface ContractorUI {
    id: string
    name: string
    nip: string
    email: string
    town: string
    postalCode: string
    street: string
    building: string
    apartment: string
}

export async function loadContractors(): Promise<ContractorUI[]> {
    const url = `${import.meta.env.VITE_WORKER_URL}/app/contractors`;
    const response = await fetch(url, {
        method: "GET",
        headers: { "Content-Type": "application/json", "X-API-Key": import.meta.env.VITE_API_KEY }
    });
    if (!response.ok)
        throw new Error(`Failed to fetch existing contractors: ${response.status}`);
    const customers: AppContractor[] = await response.json();
    return customers.map(contractorToUI);
}

function contractorToUI(customer: AppContractor): ContractorUI {
    const [town = "", postalCode = "", streetAndBuilding = ""] = customer.addressL1.split(", ");
    const match = streetAndBuilding.match(/^(.+?)\s+(\S+)$/);
    return {
        id: customer.id,
        name: customer.name,
        nip: customer.nip ?? "",
        town,
        postalCode,
        street: match?.[1] ?? streetAndBuilding,
        building: match?.[2] ?? "",
        apartment: "",
        email: ""
    };
}