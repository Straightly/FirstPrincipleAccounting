"""Dev-time backend: LLM wrapping, prompt/context assembly, artifact preparation.

Owns workflow-generation backend operations (Impl Spec §7.1). Never receives
accounting storage credentials; accounting context, when needed, is fetched
transiently through authorized runtime backend APIs and not persisted (Axiom 12).
Implemented in milestone M6.
"""
