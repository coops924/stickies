//! MCP server (stdio) exposing notes to Claude Code, Codex, or any MCP client.
//! Launched as `stickies mcp`; works whether or not the desktop app is running.

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    Implementation, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{tool, tool_handler, tool_router, ErrorData, RoleServer, ServerHandler, ServiceExt};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::store::{Note, NotePatch, Store, COLORS};

const INSTRUCTIONS: &str = "Stickies are the user's desktop sticky notes (short Markdown). \
Use list_notes or search_notes to find a note, read_note to pull its content into the conversation, \
and create_note / append_to_note to save plans, summaries or reminders for the user. \
Keep notes sticky-sized. Notes are also resources at note://<id>.";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteRef {
    /// Note id, a unique id prefix, or the note's exact title.
    pub note: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateArgs {
    /// Markdown body. The first line becomes the title.
    pub body: String,
    /// yellow, pink, green, blue, purple, orange or gray. Defaults to yellow.
    pub color: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendArgs {
    /// Note id, a unique id prefix, or the note's exact title.
    pub note: String,
    /// Markdown to add on new lines at the end of the note.
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    /// Note id, a unique id prefix, or the note's exact title.
    pub note: String,
    /// Replacement Markdown body (omit to keep the current body).
    pub body: Option<String>,
    pub color: Option<String>,
    /// Pinned notes stay above other windows.
    pub pinned: Option<bool>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Case-insensitive text to look for in note bodies, or an exact tag.
    pub query: String,
}

#[derive(Clone)]
pub struct StickiesServer {
    store: Store,
    tool_router: ToolRouter<Self>,
}

fn summary(n: &Note) -> String {
    let tags = if n.tags.is_empty() { String::new() } else { format!(" [{}]", n.tags.join(", ")) };
    let pin = if n.pinned { " (pinned)" } else { "" };
    format!("- {} | {} | {}{}{}", n.id, n.title, n.color, pin, tags)
}

fn full(n: &Note) -> String {
    format!(
        "id: {}\ntitle: {}\ncolor: {}\npinned: {}\ntags: {}\nupdated: {}\n\n{}",
        n.id,
        n.title,
        n.color,
        n.pinned,
        n.tags.join(", "),
        n.updated,
        n.body
    )
}

#[tool_router]
impl StickiesServer {
    pub fn new(store: Store) -> Self {
        StickiesServer { store, tool_router: Self::tool_router() }
    }

    #[tool(description = "List every sticky note (id | title | color | tags), most recently updated first.")]
    async fn list_notes(&self) -> Result<String, String> {
        let notes = self.store.list().map_err(|e| e.to_string())?;
        if notes.is_empty() {
            return Ok("No notes yet.".into());
        }
        Ok(notes.iter().map(summary).collect::<Vec<_>>().join("\n"))
    }

    #[tool(description = "Read the full Markdown content of one sticky note.")]
    async fn read_note(&self, Parameters(args): Parameters<NoteRef>) -> Result<String, String> {
        self.store.resolve(&args.note).map(|n| full(&n)).map_err(|e| e.to_string())
    }

    #[tool(description = "Search sticky notes by text or tag.")]
    async fn search_notes(&self, Parameters(args): Parameters<SearchArgs>) -> Result<String, String> {
        let notes = self.store.search(&args.query).map_err(|e| e.to_string())?;
        if notes.is_empty() {
            return Ok(format!("No notes match '{}'.", args.query));
        }
        Ok(notes.iter().map(summary).collect::<Vec<_>>().join("\n"))
    }

    #[tool(description = "Create a new sticky note. It appears on the user's desktop if Stickies is running.")]
    async fn create_note(&self, Parameters(args): Parameters<CreateArgs>) -> Result<String, String> {
        let note = self
            .store
            .create(&args.body, args.color.as_deref(), args.tags.unwrap_or_default())
            .map_err(|e| e.to_string())?;
        Ok(format!("Created note {} \"{}\".", note.id, note.title))
    }

    #[tool(description = "Append Markdown lines to the end of an existing sticky note.")]
    async fn append_to_note(&self, Parameters(args): Parameters<AppendArgs>) -> Result<String, String> {
        let note = self.store.append(&args.note, &args.text).map_err(|e| e.to_string())?;
        Ok(format!("Appended to note {} \"{}\".", note.id, note.title))
    }

    #[tool(description = "Replace a sticky note's body, color, pinned state or tags.")]
    async fn update_note(&self, Parameters(args): Parameters<UpdateArgs>) -> Result<String, String> {
        let patch = NotePatch { body: args.body, color: args.color, pinned: args.pinned, tags: args.tags };
        let note = self.store.update(&args.note, patch).map_err(|e| e.to_string())?;
        Ok(format!("Updated note {} \"{}\".", note.id, note.title))
    }

    #[tool(description = "Move a sticky note to the trash. Only do this when the user asks.")]
    async fn delete_note(&self, Parameters(args): Parameters<NoteRef>) -> Result<String, String> {
        let note = self.store.delete(&args.note).map_err(|e| e.to_string())?;
        Ok(format!("Moved note \"{}\" to the trash.", note.title))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for StickiesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new("stickies", env!("CARGO_PKG_VERSION")))
            .with_instructions(format!("{INSTRUCTIONS} Colors: {}.", COLORS.join(", ")))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let notes = self.store.list().map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let resources = notes
            .iter()
            .map(|n| {
                Resource::new(format!("note://{}", n.id), n.title.clone())
                    .with_title(n.title.clone())
                    .with_mime_type("text/markdown")
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let id = request
            .uri
            .strip_prefix("note://")
            .ok_or_else(|| ErrorData::resource_not_found(format!("unknown resource {}", request.uri), None))?;
        let note = self
            .store
            .resolve(id)
            .map_err(|e| ErrorData::resource_not_found(e.to_string(), None))?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(note.body, request.uri.clone())]).into())
    }
}

pub async fn serve_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open_default()?;
    let server = StickiesServer::new(store).serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}
