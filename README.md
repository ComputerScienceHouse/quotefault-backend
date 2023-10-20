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
* `page={num}` - The page index (default: 0)
* `limit={num}` - The maximum number of entries to return (default: 10)
* `submitter={username}` - Filters for quotes submitted by a certain user
* `speaker={username}` - Filters for quotes said by a certain user

### PUT /api/hide/{qid}

Hides a quote

### PUT /api/unhide/{qid}

Unhides a quote

### POST /api/report/{qid}

Reports a quote

### GET /api/reports

Returns a list of quotes which 
