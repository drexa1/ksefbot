import {D1Driver, Repository} from "../repository/d1";
import {UsersService} from "./app/users";
import {Env} from "../worker";
import {InvoicesService} from "./app/invoices";
import {ContractorsService} from "./app/contractors";
import {TaxesService} from "./app/taxes";
import {KsefService} from "./ksef/ksef";

export let service!: Services;
export const initServices = (env: Env) => service ??= new Services(new Repository(new D1Driver(env.D1)));

export class Services {

    readonly users = new UsersService(this.repo);
    readonly contractors = new ContractorsService(this.repo);
    readonly invoices = new InvoicesService(this.repo);
    readonly taxes = new TaxesService(this.repo);

    readonly ksef = new KsefService(this.repo);

    constructor(private readonly repo: Repository) {}
}