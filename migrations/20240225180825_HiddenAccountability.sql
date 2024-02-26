-- Add migration script here
CREATE TABLE public.hidden (
  quote_id integer PRIMARY KEY NOT NULL,
  reason text NOT NULL CHECK(char_length(reason) >= 10),
  actor character varying(32) NOT NULL,
  CONSTRAINT fk_quote FOREIGN KEY(quote_id) REFERENCES public.quotes(id) ON DELETE SET NULL
);

INSERT INTO public.hidden(quote_id, reason, actor) (
  SELECT id as quote_id, 'No reason given' as reason, 'testing' as actor FROM public.quotes WHERE hidden = true
);

ALTER TABLE public.quotes RENAME COLUMN hidden TO hidden_bool;
ALTER TABLE public.quotes ADD COLUMN hidden integer;
ALTER TABLE public.quotes ADD CONSTRAINT quote_hidden FOREIGN KEY(hidden) REFERENCES public.hidden(quote_id) ON DELETE CASCADE;
UPDATE public.quotes SET hidden=id WHERE hidden_bool=true;
ALTER TABLE public.quotes DROP COLUMN hidden_bool;
