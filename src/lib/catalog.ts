// Catálogo dos serviços conectáveis: o que o usuário vê em "Descobrir", com nome,
// categoria, descrição, o que o assistente poderá fazer e o ícone da marca.
// Os ícones em `static/connectors` são de uma cor (Simple Icons, domínio público) e
// pintados com a cor da marca; os marcados `own` têm as cores do próprio fabricante.

import type { AccountConnector } from "./types";

export type Category = "dev" | "product" | "docs" | "data" | "design" | "payments" | "sales" | "support" | "marketing" | "automation" | "social" | "email";

export const CATEGORY_ORDER: Category[] = ["dev", "product", "docs", "data", "design", "payments", "sales", "support", "marketing", "automation", "social", "email"];

export const CATEGORY_LABEL: Record<Category, string> = {
  dev: "Desenvolvimento",
  product: "Produto",
  docs: "Docs",
  data: "Dados",
  design: "Design",
  payments: "Pagamentos",
  sales: "Vendas",
  support: "Suporte",
  marketing: "Marketing",
  automation: "Automação",
  social: "Redes sociais",
  email: "Email",
};

export interface CatalogEntry {
  /** Arquivo em `static/connectors`, sem extensão. */
  icon?: string;
  /** Cor da marca para pintar o ícone; sem cor usa o tom do texto. */
  color?: string;
  /** O ícone já vem nas cores do fabricante (não é pintado). */
  own?: boolean;
  category: Category;
  description: string;
  /** O que o assistente poderá fazer; a última linha com `asks` sempre pede aprovação. */
  can: string[];
  asks?: string;
  /** Produtos e termos pelos quais o usuário procura o serviço. */
  aliases?: string[];
}

/** Chave do catálogo: nome do conector em minúsculas. */
export const CATALOG: Record<string, CatalogEntry> = {
  atlassian: {
    icon: "atlassian",
    color: "#0052CC",
    category: "dev",
    description: "Issues do Jira e páginas do Confluence.",
    can: ["Ler e criar issues do Jira", "Ler e editar páginas do Confluence"],
    aliases: ["jira", "confluence", "bitbucket", "tickets", "issues"],
  },
  github: {
    icon: "github",
    category: "dev",
    description: "Código, issues e pull requests dos seus repositórios.",
    can: ["Ler código, issues e pull requests", "Abrir pull requests e comentar"],
    asks: "Por padrão, merge e deploy pedem sua aprovação",
    aliases: ["git", "repositório", "repositorio", "pull request"],
  },
  gitlab: { icon: "gitlab", color: "#FC6D26", category: "dev", description: "Código, issues e merge requests.", can: ["Ler código, issues e merge requests", "Abrir merge requests e comentar"], aliases: ["git"] },
  context7: { category: "dev", description: "Documentação atualizada de bibliotecas e frameworks.", can: ["Buscar a documentação de uma biblioteca", "Ler exemplos de código atualizados"], aliases: ["docs", "documentação", "documentacao", "biblioteca"] },
  sentry: { icon: "sentry", category: "dev", description: "Erros e eventos dos seus apps.", can: ["Ver erros e eventos", "Ver releases"], aliases: ["erros", "crash", "monitoramento"] },
  vercel: { icon: "vercel", category: "dev", description: "Projetos, deploys e logs na Vercel.", can: ["Ver projetos, deploys e logs"], aliases: ["deploy", "hospedagem"] },
  netlify: { icon: "netlify", color: "#00C7B7", category: "dev", description: "Sites e deploys na Netlify.", can: ["Ver sites e deploys", "Fazer deploy e mudar configurações"], aliases: ["deploy", "hospedagem"] },
  cloudflare: { icon: "cloudflare", color: "#F38020", category: "dev", description: "Workers, armazenamento e observabilidade.", can: ["Ver Workers, KV, R2 e D1", "Mudar Workers e armazenamento"], asks: "Por padrão, deploys pedem sua aprovação", aliases: ["workers", "dns", "cdn"] },
  supabase: { icon: "supabase", color: "#3FCF8E", category: "data", description: "Projetos, banco e funções no Supabase.", can: ["Ver projetos e funções", "Consultar e alterar o banco"], asks: "Por padrão, deploys pedem sua aprovação", aliases: ["banco", "postgres", "sql"] },
  neon: { icon: "neon", color: "#00E599", category: "data", description: "Bancos Postgres sem servidor.", can: ["Ver projetos e consultar o banco", "Criar branches e alterar o banco"], aliases: ["postgres", "sql", "banco"] },
  prisma: { icon: "prisma", category: "data", description: "Bancos e dados com o Prisma.", can: ["Ver bancos e consultar", "Criar bancos e alterar dados"], aliases: ["orm", "banco"] },
  airtable: { icon: "airtable", color: "#18BFFF", category: "data", description: "Bases e registros do Airtable.", can: ["Ler bases e registros", "Criar e editar registros"], aliases: ["planilha", "base"] },
  postman: { icon: "postman", color: "#FF6C37", category: "dev", description: "Coleções e APIs no Postman.", can: ["Ver coleções e APIs", "Criar e editar coleções"], aliases: ["api"] },
  datadog: { icon: "datadog", color: "#632CA6", category: "dev", description: "Logs, métricas e monitores.", can: ["Buscar logs e métricas", "Ver monitores e incidentes"], aliases: ["logs", "monitoramento"] },
  grafana: { icon: "grafana", color: "#F46800", category: "dev", description: "Dashboards, métricas e alertas.", can: ["Ver dashboards, métricas e logs", "Criar dashboards e alertas"], aliases: ["dashboards", "métricas", "metricas"] },

  linear: { icon: "linear", color: "#5E6AD2", category: "product", description: "Issues, projetos e ciclos do seu time.", can: ["Ler e criar issues", "Mudar status, projetos e ciclos"], aliases: ["issues", "tarefas", "bugs"] },
  asana: { icon: "asana", color: "#F06A6A", category: "product", description: "Tarefas e projetos do time.", can: ["Ler e criar tarefas", "Atualizar projetos e responsáveis"], aliases: ["tarefas", "projetos"] },
  "monday.com": { icon: "monday", category: "product", description: "Quadros e itens do monday.com.", can: ["Ler quadros e itens", "Criar e atualizar itens"], aliases: ["monday", "tarefas", "quadro"] },
  clickup: { icon: "clickup", color: "#7B68EE", category: "product", description: "Tarefas e docs no ClickUp.", can: ["Ler tarefas e docs", "Criar e atualizar tarefas"], aliases: ["tarefas"] },
  todoist: { icon: "todoist", color: "#E44332", category: "product", description: "Suas tarefas e projetos.", can: ["Ver tarefas e projetos", "Criar e concluir tarefas"], aliases: ["tarefas", "lista"] },
  notion: { icon: "notion", category: "docs", description: "Páginas, wikis e bancos de dados.", can: ["Buscar e ler páginas", "Criar e editar páginas e bancos de dados"], aliases: ["notas", "wiki", "páginas", "paginas"] },
  "google docs": { icon: "googledocs", color: "#4285F4", category: "docs", description: "Documentos no Google Docs.", can: ["Buscar e ler documentos", "Criar e editar documentos"], aliases: ["documento", "docs", "word"] },
  "google sheets": { icon: "googlesheets", color: "#34A853", category: "data", description: "Planilhas no Google Sheets.", can: ["Ler planilhas", "Adicionar linhas e atualizar células"], aliases: ["planilha", "sheets", "excel"] },
  "google slides": { icon: "googleslides", color: "#F4B400", category: "docs", description: "Apresentações no Google Slides.", can: ["Ler apresentações", "Criar e editar slides"], aliases: ["apresentação", "apresentacao", "slides", "powerpoint"] },
  "google drive": { icon: "googledrive", color: "#4285F4", category: "docs", description: "Arquivos e pastas no seu Drive.", can: ["Buscar e ler arquivos", "Criar e editar documentos"], asks: "Compartilhar sempre pede sua aprovação", aliases: ["arquivos", "drive", "nuvem"] },
  "google calendar": { icon: "googlecalendar", color: "#4285F4", category: "product", description: "Agendas, eventos e horários livres.", can: ["Ver agendas, eventos e horários livres"], asks: "Criar ou mudar eventos sempre pede sua aprovação", aliases: ["agenda", "calendário", "calendario", "reunião", "reuniao"] },
  gmail: { icon: "gmail", color: "#EA4335", category: "email", description: "Seus emails no Gmail.", can: ["Ler e buscar emails", "Criar rascunhos e organizar em pastas"], asks: "Enviar sempre pede sua aprovação", aliases: ["email", "e-mail", "mail", "correio"] },
  box: { icon: "box", color: "#0061D5", category: "docs", description: "Arquivos e pastas no Box.", can: ["Buscar e ler arquivos", "Enviar e organizar arquivos"], aliases: ["arquivos", "nuvem"] },
  dropbox: { icon: "dropbox", color: "#0061FF", category: "docs", description: "Arquivos e pastas no Dropbox.", can: ["Buscar e ler arquivos", "Enviar e organizar arquivos"], aliases: ["arquivos", "nuvem"] },
  slack: { icon: "slack", own: true, category: "product", description: "Canais, mensagens e threads do time.", can: ["Ler canais, mensagens e threads"], asks: "Enviar mensagens sempre pede sua aprovação", aliases: ["chat", "mensagens", "canal"] },
  "microsoft teams": { category: "product", description: "Canais e chats no Teams.", can: ["Ler canais e chats"], asks: "Enviar mensagens sempre pede sua aprovação", aliases: ["teams", "chat"] },
  miro: { icon: "miro", category: "product", description: "Quadros colaborativos no Miro.", can: ["Ler quadros", "Adicionar notas, formas e diagramas"], aliases: ["quadro", "diagrama"] },

  figma: { icon: "figma", category: "design", description: "Designs, componentes e variáveis.", can: ["Ver designs, componentes e variáveis"], aliases: ["design", "protótipo", "prototipo", "ui"] },
  canva: { icon: "canva", own: true, category: "design", description: "Designs e artes no Canva.", can: ["Ver e criar designs"], aliases: ["design", "arte", "post"] },
  runway: { category: "design", description: "Vídeos e imagens gerados no Runway.", can: ["Gerar vídeos e imagens", "Ver os projetos"], aliases: ["vídeo", "video", "imagem", "ia"] },
  gamma: { icon: "gamma", category: "docs", description: "Apresentações e documentos no Gamma.", can: ["Criar apresentações e documentos"], aliases: ["apresentação", "apresentacao"] },

  stripe: { icon: "stripe", color: "#635BFF", category: "payments", description: "Clientes, pagamentos e assinaturas.", can: ["Ver clientes, pagamentos e assinaturas"], asks: "Reembolsos e mudanças sempre pedem sua aprovação", aliases: ["pagamentos", "assinaturas", "cobrança", "cobranca"] },
  paypal: { icon: "paypal", own: true, category: "payments", description: "Pedidos, transações e faturas.", can: ["Ver pedidos e transações", "Criar faturas"], aliases: ["pagamentos"] },
  square: { icon: "square", category: "payments", description: "Pagamentos, pedidos e catálogo.", can: ["Ver pagamentos, pedidos e catálogo"], aliases: ["pagamentos"] },
  "mercado pago": { icon: "mercadopago", color: "#00B1EA", category: "payments", description: "Pagamentos no Mercado Pago.", can: ["Ler a documentação e checar a integração", "Ver pagamentos"], aliases: ["mercadopago", "pix", "pagamentos"] },

  hubspot: { icon: "hubspot", color: "#FF7A59", category: "sales", description: "Contatos, empresas e negócios no CRM.", can: ["Ver contatos, empresas e negócios", "Criar e atualizar registros, notas e tarefas"], aliases: ["crm", "vendas", "leads"] },
  salesforce: { icon: "salesforce", own: true, category: "sales", description: "Contas, leads e oportunidades.", can: ["Ver contas, contatos, leads e oportunidades", "Criar e atualizar registros"], aliases: ["crm", "vendas", "leads"] },
  attio: { icon: "attio", category: "sales", description: "CRM com contatos e negócios.", can: ["Ver contatos, empresas e negócios", "Criar e atualizar registros e notas"], aliases: ["crm"] },
  close: { icon: "close", category: "sales", description: "Leads e oportunidades no Close.", can: ["Ver leads e oportunidades", "Criar e atualizar leads e atividades"], aliases: ["crm", "leads"] },
  calendly: { icon: "calendly", color: "#006BFF", category: "sales", description: "Agendamentos e disponibilidade.", can: ["Ver eventos e disponibilidade", "Criar links de agendamento"], aliases: ["agenda", "agendamento"] },

  intercom: { icon: "intercom", category: "support", description: "Conversas e contatos com clientes.", can: ["Ler conversas e contatos"], asks: "Respostas sempre pedem sua aprovação", aliases: ["suporte", "atendimento", "chat"] },
  zendesk: { icon: "zendesk", category: "support", description: "Tickets e comentários de suporte.", can: ["Ler tickets e comentários"], asks: "Criar, mudar e responder tickets sempre pede sua aprovação", aliases: ["suporte", "tickets"] },

  klaviyo: { icon: "klaviyo", category: "marketing", description: "Campanhas, fluxos e métricas.", can: ["Ver campanhas, fluxos e métricas", "Criar campanhas e gerenciar perfis"], aliases: ["email marketing"] },
  semrush: { icon: "semrush", color: "#FF642D", category: "marketing", description: "SEO: palavras-chave e backlinks.", can: ["Pesquisar palavras-chave, domínios e backlinks"], aliases: ["seo"] },
  ahrefs: { icon: "ahrefs", color: "#FF8800", category: "marketing", description: "SEO: palavras-chave e sites.", can: ["Pesquisar palavras-chave, backlinks e sites"], aliases: ["seo"] },
  webflow: { icon: "webflow", color: "#146EF5", category: "marketing", description: "Sites e CMS no Webflow.", can: ["Ler e editar o CMS"], asks: "Publicar sempre pede sua aprovação", aliases: ["site", "cms"] },
  posthog: { icon: "posthog", category: "data", description: "Produto: insights, erros e experimentos.", can: ["Ver insights, erros e experimentos", "Criar insights e mudar feature flags"], aliases: ["analytics", "métricas", "metricas"] },
  mixpanel: { icon: "mixpanel", color: "#7856FF", category: "data", description: "Eventos, relatórios e funis.", can: ["Consultar eventos, relatórios e funis"], aliases: ["analytics"] },
  amplitude: { icon: "amplitude", color: "#1E61F0", category: "data", description: "Gráficos e dashboards de produto.", can: ["Ver gráficos e dashboards", "Criar gráficos"], aliases: ["analytics"] },

  zapier: { icon: "zapier", color: "#FF4F00", category: "automation", description: "As ações que você ligou no Zapier.", can: ["Rodar as ações que você ligou no Zapier"], aliases: ["automação", "automacao", "zap"] },
  fireflies: { icon: "fireflies", color: "#DA4292", category: "product", description: "Transcrições e resumos de reuniões.", can: ["Buscar e ler transcrições e resumos"], aliases: ["reunião", "reuniao", "transcrição", "transcricao"] },
  granola: { icon: "granola", category: "product", description: "Notas de reunião.", can: ["Buscar e ler notas de reunião"], aliases: ["reunião", "reuniao", "notas"] },

  instagram: { icon: "instagram", category: "social", description: "Posts, stories e mensagens do Direct.", can: ["Ver posts, stories e métricas", "Publicar posts e ler mensagens do Direct"], asks: "Responder mensagens sempre pede sua aprovação", aliases: ["rede social"] },
  facebook: { icon: "facebook", color: "#0866FF", category: "social", description: "Posts, comentários e métricas da página.", can: ["Ver posts, comentários e métricas", "Publicar posts, fotos e vídeos e responder comentários"], asks: "Apagar posts e comentários sempre pede sua aprovação", aliases: ["rede social"] },
  linkedin: { icon: "linkedin", color: "#0A66C2", category: "social", description: "Posts e comentários no LinkedIn.", can: ["Publicar posts, links e vídeos", "Comentar em posts"], asks: "Apagar posts sempre pede sua aprovação", aliases: ["rede social"] },
  youtube: { icon: "youtube", color: "#FF0000", category: "social", description: "Vídeos, comentários e métricas do canal.", can: ["Ver vídeos, comentários e métricas", "Editar vídeos e playlists e comentar"], asks: "Responder, moderar e apagar comentários sempre pede sua aprovação", aliases: ["vídeo", "video", "canal"] },
  whatsapp: { icon: "whatsapp", color: "#25D366", category: "social", description: "Números, modelos e entregas da API do WhatsApp.", can: ["Ver números, modelos e entregas", "Criar modelos de mensagem"], asks: "Enviar mensagens sempre pede sua aprovação", aliases: ["mensagens", "zap"] },
};

/** Serviços internos do claude.ai que não são conectores de terceiros. */
const INTERNAL = ["claude data", "claude docs"];

const normalize = (name: string) => name.trim().toLowerCase();

export function catalogOf(c: Pick<AccountConnector, "display_name">): CatalogEntry | undefined {
  return CATALOG[normalize(c.display_name)];
}

/** Conectores da conta que o usuário escolhe em Descobrir (fora os internos do claude.ai). */
export function isListed(c: AccountConnector): boolean {
  return c.source === "account" && !INTERNAL.includes(normalize(c.display_name));
}

/** Descrição do conector: a do catálogo ou uma genérica. */
export function describe(c: AccountConnector): string {
  return catalogOf(c)?.description ?? (c.source === "plugin" ? "Fornecido por um plugin." : "Serviço conectado à sua conta claude.ai.");
}

/** O conector atende à busca pelo nome, pela descrição, pela categoria ou por um produto que ele cobre. */
export function matchesQuery(c: AccountConnector, query: string): boolean {
  const q = normalize(query);
  if (!q) return true;
  const entry = catalogOf(c);
  const haystack = [c.display_name, entry?.description ?? "", entry ? CATEGORY_LABEL[entry.category] : "", ...(entry?.aliases ?? [])].join(" ").toLowerCase();
  return haystack.includes(q) || (entry?.aliases ?? []).some((alias) => q.includes(alias));
}

/** Sugestões, na ordem: as primeiras ainda não vinculadas entram em destaque. */
export const SUGGESTED = ["github", "atlassian", "gmail", "google drive", "notion", "linear", "slack", "google calendar"];
