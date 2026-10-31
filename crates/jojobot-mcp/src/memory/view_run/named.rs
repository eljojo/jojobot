use super::*;
use jojobot_domain::memory::{Entity, FormerHandle, kinds, types::DeclaredType, view_inputs};
use jojobot_query_api::{Board, ViewAnswer, ViewOutcome, ViewRequest};

pub(crate) async fn read_view(
    memory: &dyn Memory,
    view: &EntityId,
) -> Result<graph::Asked, MemoryError> {
    let held = memory.fields(view).await?;
    let facts = memory.recall(view).await?;
    Ok(graph::asked_by_view(&held, &facts))
}

pub(crate) struct Binding {
    pub known: Vec<Entity>,
    pub former: Vec<FormerHandle>,
}

impl Binding {
    pub(crate) async fn read(memory: &dyn Memory) -> Result<Self, (&'static str, MemoryError)> {
        Ok(Self {
            known: memory
                .list_entities(None)
                .await
                .map_err(|e| ("the things it names", e))?,
            former: memory
                .former_handles()
                .await
                .map_err(|e| ("the former handles", e))?,
        })
    }
}

pub(crate) fn bind_view(
    asked: graph::Asked,
    view: &EntityId,
    given: &BTreeMap<String, String>,
    binding: &Binding,
    today: jiff::civil::Date,
) -> Result<(graph::Asked, Option<EntityId>), view_inputs::Refusal> {
    asked.filled(view.as_str(), given, &binding.known, &binding.former, today)
}

pub(super) async fn prepare(
    memory: &dyn Memory,
    request: ViewRequest,
) -> Result<(Question, ViewAnswer), Failed> {
    let view = request.view;
    let asked = read_view(memory, &view).await.map_err(Failed::Selection)?;
    let binding = Binding::read(memory)
        .await
        .map_err(|(part, error)| Failed::Preparation(part, error))?;
    // The instance owns the frame for every read whose caller states no zone.
    let frame = crate::orientation::instance_zone::unzoned_frame(memory).await;
    let today = request.clock.today_in(&frame);
    let declared = view_inputs::parse(asked.inputs.as_deref().unwrap_or(""));
    let mut meta = ViewAnswer {
        inputs: declared.clone().unwrap_or_default(),
        choices: binding.known.clone(),
        shown: Vec::new(),
        ready: asked.ready,
        outcome: ViewOutcome::Answered,
        board: None,
    };
    let question = async {
        let declared = declared.map_err(|e| MemoryError::InvalidQuery(e.words))?;
        // Only declared URL fields are inputs; a tracking parameter is not a call argument.
        let given: BTreeMap<_, _> = request
            .inputs
            .into_iter()
            .filter(|(name, value)| {
                declared.iter().any(|d| d.name == *name) && !value.trim().is_empty()
            })
            .collect();
        if given.is_empty()
            && view_inputs::bind(
                view.as_str(),
                &declared,
                &given,
                &binding.known,
                &binding.former,
                today,
            )
            .is_err()
        {
            meta.outcome = ViewOutcome::NeedsInputs;
        }
        let (asked, parent) = bind_view(asked, &view, &given, &binding, today)
            .map_err(|e| MemoryError::InvalidQuery(e.words))?;
        let kind = asked
            .selects
            .as_deref()
            .map(|token| kinds::resolve(token.trim()))
            .transpose()
            .map_err(|why| MemoryError::InvalidQuery(why.to_string()))?;
        if asked.ready.is_some() && kind.is_some_and(|k| k != EntityKind::WORK) {
            return Err(MemoryError::InvalidQuery(
                "it asks what is ready, which only work answers.".into(),
            ));
        }
        let answers_type = match asked.answers_type.as_deref() {
            None => None,
            Some(name) => Some(
                DeclaredType::named(&memory.declared_types().await?, name)
                    .cloned()
                    .ok_or_else(|| {
                        MemoryError::InvalidQuery(format!("no declared type is called '{name}'"))
                    })?,
            ),
        };
        meta.shown = asked.keys.clone().unwrap_or_default();
        let mut question = Question::of_view(&view, &asked, kind, answers_type, parent, today)?;
        // The board annotation belongs to this named view, never to every work query.
        question.view = Some(ViewRequest {
            view: view.clone(),
            inputs: Default::default(),
            clock: request.clock,
        });
        Ok(question)
    }
    .await;
    match question {
        Ok(question) => Ok((question, meta)),
        Err(error) => {
            if matches!(meta.outcome, ViewOutcome::Answered) {
                meta.outcome = match error {
                    MemoryError::Store(_) | MemoryError::StoredDamage { .. } => {
                        ViewOutcome::Unreadable("the declared types", error)
                    }
                    MemoryError::ViewNarrowsNothing { view } => {
                        ViewOutcome::Refused(super::narrows_nothing_words(&view))
                    }
                    error => ViewOutcome::Refused(error.to_string()),
                };
            }
            Ok((Question::about(view, None, None), meta))
        }
    }
}

pub(super) fn is_board(question: &Question) -> bool {
    question
        .view
        .as_ref()
        .is_some_and(|v| v.view.as_str() == "view:board")
}

pub(crate) async fn board_of(
    memory: &dyn Memory,
    parent: Option<&EntityId>,
    objects: &[graph::Object],
) -> Result<Board, MemoryError> {
    // An unreadable instance is not an instance that mapped no status words (rule 130).
    let instance = crate::orientation::instance_zone::instance_fields_of(memory).await?;
    let project = match parent {
        Some(id) => Some(memory.fields(id).await?),
        None => None,
    };
    let mut status_words = kinds::accepted_words(&instance, project.as_ref());
    let mut finished = std::collections::BTreeSet::new();
    for object in objects {
        let word = object
            .fields
            .get(kinds::STATUS)
            .cloned()
            .unwrap_or_default();
        if !status_words.contains(&word) {
            status_words.push(word);
        }
        if kinds::is_finished(object.entity.kind.as_token(), &object.fields, &instance) {
            finished.insert(object.entity.id.clone());
        }
    }
    Ok(Board {
        status_words,
        done: finished.len(),
        finished,
    })
}
