# Quotefault Backend

## API

### POST /api/quote

Creates a quote

#### Data

Probably some stringified JSON array containing the quote, submitter, and speaker

### GET /api/quotes

Queries a list of quotes. With no parameters it returns the most recent 10 quotes.

#### Params

* `q={query}` - Searches the quotes for a list of space separates keywords
* `offset={num}` - The page index (default: 0)
* `limit={num}` - The maximum number of entries to return (default: 10)
* `submitter={username}` - Filters for quotes submitted by a certain user
* `speaker={username}` - Filters for quotes said by a certain user

### PUT /api/hide/{qid}

Hides a quote

### PUT /api/unhide/{qid}

Unhides a quote

### PUT /api/report/{qid}

Reports a quote

### GET /api/reports

Returns a list of quotes which

## Database Schema

### Quotes Table

```SQL
CREATE TABLE Quotes (
    id INT4 PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    submitter VARCHAR(32) NOT NULL,
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    hidden BOOL NOT NULL DEFAULT FALSE,
    reported BOOL NOT NULL DEFAULT FALSE
);
```

### Quote Shards Table

```SQL
CREATE TABLE Shards (
    quote_id INT4 REFERENCES quotes(id) NOT NULL,
    index SMALLINT NOT NULL,
    body TEXT NOT NULL,
    speaker VARCHAR(32) NOT NULL,
    PRIMARY KEY (quote_id, index)
);
```
