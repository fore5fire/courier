use crate::{Error, Result, State};
use cel_interpreter::{Context, Program};
use indexmap::IndexMap;
use std::{collections::HashMap, convert::Infallible, error::Error as StdError, iter::repeat};
use toml::Table;

#[derive(Debug)]
pub struct Plan {
    pub steps: IndexMap<String, Step>,
}

impl<'a> Plan {
    pub fn parse(input: &'a str) -> Result<Self> {
        let parsed = toml::from_str(input).map_err(|e| Error(e.to_string()))?;
        Self::from_value(parsed)
    }

    pub fn from_value(mut table: Table) -> Result<Self> {
        // Remove the special courier table.
        let mut defaults = match table.remove("courier") {
            Some(toml::Value::Table(mut courier)) => courier
                .remove("defaults")
                .unwrap_or(toml::Value::Table(Table::default())),
            Some(_) => return Err(Error::from("invalid type for courier table")),
            None => toml::Value::Table(Table::default()),
        };

        // Apply the implicit defaults to the user defaults.
        merge_toml(
            &mut defaults,
            &toml::Value::Table(
                Table::try_from(HashMap::from([
                    (
                        "http",
                        toml::Value::from(HashMap::from([("method", toml::Value::from("GET"))])),
                    ),
                    (
                        "graphql",
                        toml::Value::from(HashMap::from([(
                            "http",
                            HashMap::from([
                                ("method", toml::Value::from("POST")),
                                (
                                    "headers",
                                    toml::Value::from(HashMap::from([(
                                        "Content-Type",
                                        "application/json",
                                    )])),
                                ),
                            ]),
                        )])),
                    ),
                ]))
                .map_err(|e| Error(e.to_string()))?,
            ),
        );

        // Parse all remaining tables as steps.
        let steps: IndexMap<String, Step> = table
            .into_iter()
            .map(|(name, value)| {
                let toml::Value::Table(t) = value else {
                    return Err(Error(format!("invalid type for {}", name)));
                };
                Ok(Some((
                    name,
                    Step::from_table(t, &defaults.as_table().unwrap())?,
                )))
            })
            .filter_map(Result::transpose)
            .collect::<Result<_>>()?;

        Ok(Plan { steps })
    }
}

fn merge_toml(target: &mut toml::Value, defaults: &toml::Value) {
    let (toml::Value::Table(target), toml::Value::Table(defaults)) = (target, defaults) else {
        return;
    };
    for (name, default) in defaults {
        match target.entry(name) {
            toml::map::Entry::Vacant(entry) => {
                entry.insert(default.clone());
            }
            toml::map::Entry::Occupied(mut entry) => {
                merge_toml(entry.get_mut(), default);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TLSVersion {
    SSL1,
    SSL2,
    SSL3,
    TLS1_0,
    TLS1_1,
    TLS1_2,
    TLS1_3,
}

impl TryFrom<String> for TLSVersion {
    type Error = Error;
    fn try_from(value: String) -> Result<Self> {
        match value.as_str() {
            "SSL1" => Ok(TLSVersion::SSL1),
            "SSL2" => Ok(TLSVersion::SSL2),
            "SSL3" => Ok(TLSVersion::SSL3),
            "TLS1_0" => Ok(TLSVersion::TLS1_0),
            "TLS1_1" => Ok(TLSVersion::TLS1_1),
            "TLS1_2" => Ok(TLSVersion::TLS1_2),
            "TLS1_3" => Ok(TLSVersion::TLS1_3),
            _ => Err(Error("invalid TLS version".to_owned())),
        }
    }
}

#[derive(Debug)]
pub enum HTTPVersion {
    HTTP0_9,
    HTTP1_0,
    HTTP1_1,
    HTTP2,
    HTTP3,
}

#[derive(Debug)]
pub struct Header {
    key: String,
    value: String,
}

#[derive(Debug, Default)]
pub struct HTTPRequest {
    pub url: PlanValue<Infallible, String>,
    pub body: PlanValue<Infallible, String>,
    pub options: HTTPOptions,
    pub tls: TLSOptions,
    pub ip: IPOptions,
}

impl TryFrom<toml::Value> for HTTPRequest {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(HTTPRequest {
            url: protocol
                .remove("url")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("http.url is required"))?,
            body: protocol
                .remove("body")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .unwrap_or_default(),
            tls: protocol
                .remove("tls")
                .map(TLSOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
            ip: protocol
                .remove("ip")
                .map(IPOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
            // The protocol's own options are sourced from the same level as the request.
            options: HTTPOptions::try_from(toml::Value::Table(protocol))?,
        })
    }
}

#[derive(Debug, Default)]
pub struct HTTP1Request {
    pub http: HTTPRequest,
    pub tls: TLSOptions,
    pub tcp: TCPOptions,
    pub ip: IPOptions,
}

#[derive(Debug, Default)]
pub struct HTTP2Request {
    pub http: HTTPRequest,
    pub tls: TLSOptions,
    pub tcp: TCPOptions,
    pub ip: IPOptions,
}

#[derive(Debug, Default)]
pub struct HTTP3Request {
    pub http: HTTPRequest,
    pub tls: TLSOptions,
    pub quic: QUICOptions,
    pub udp: UDPOptions,
    pub ip: IPOptions,
}

#[derive(Debug, Default)]
pub struct GraphQLRequest {
    pub url: PlanValue<Infallible, String>,
    pub query: PlanValue<Infallible, String>,
    pub params: PlanValueTable,
    pub operation: Option<PlanValue<Infallible, String>>,
    //pub use_body: PlanValue<Infallible, bool>,
    pub websocket: WebsocketOptions,
    pub http: HTTPOptions,
    pub tls: TLSOptions,
    pub ip: IPOptions,
}

impl TryFrom<toml::Value> for GraphQLRequest {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(GraphQLRequest {
            url: protocol
                .remove("url")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("graphql.url is required"))?,
            query: protocol
                .remove("query")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("graphql.query is required"))?,
            params: protocol
                .remove("params")
                .map(PlanValueTable::try_from)
                .transpose()?
                .unwrap_or_default(),
            operation: protocol
                .remove("operation")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten(),
            websocket: protocol
                .remove("websocket")
                .map(WebsocketOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
            http: protocol
                .remove("http")
                .map(HTTPOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
            tls: protocol
                .remove("tls")
                .map(TLSOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
            ip: protocol
                .remove("ip")
                .map(IPOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct HTTPOptions {
    pub method: PlanValue<Infallible, String>,
    pub headers: PlanValueTable,
}

impl TryFrom<toml::Value> for HTTPOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(HTTPOptions {
            method: protocol
                .remove("method")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("http.method is required"))?,
            headers: protocol
                .remove("headers")
                .map(PlanValueTable::try_from)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct WebsocketOptions {}

impl TryFrom<toml::Value> for WebsocketOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(WebsocketOptions {})
    }
}

#[derive(Debug, Default, Clone)]
pub struct TLSOptions {
    pub version: Option<PlanValue<Error, TLSVersion>>,
}

impl TryFrom<toml::Value> for TLSOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error::from("invalid type"));
        };
        Ok(Self {
            version: protocol
                .remove("version")
                .map(|v| PlanValue::try_from_value(v))
                .transpose()?
                .flatten(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct TCPRequest {
    pub body: PlanValue<Infallible, Vec<u8>>,
    pub host: PlanValue<Infallible, String>,
    pub port: PlanValue<Infallible, String>,
    pub options: TCPOptions,
}

impl TryFrom<toml::Value> for TCPRequest {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(Self {
            host: protocol
                .remove("host")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("tcp.host is required"))?,
            port: protocol
                .remove("port")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("tcp.port is required"))?,
            body: protocol
                .remove("body")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .unwrap_or_default(),
            options: protocol
                .remove("options")
                .map(TCPOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct TLSRequest {
    pub port: PlanValue<Infallible, String>,
    pub body: PlanValue<Infallible, Vec<u8>>,
    pub host: PlanValue<Infallible, String>,
    pub options: TCPOptions,
}

impl TryFrom<toml::Value> for TLSRequest {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error("invalid type".to_owned()));
        };
        Ok(Self {
            host: protocol
                .remove("host")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("tls.host is required"))?,
            port: protocol
                .remove("port")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .ok_or_else(|| Error::from("tls.port is required"))?,
            body: protocol
                .remove("body")
                .map(PlanValue::try_from_value)
                .transpose()?
                .flatten()
                .unwrap_or_default(),
            options: protocol
                .remove("options")
                .map(TCPOptions::try_from)
                .transpose()?
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct TCPOptions {}

impl TryFrom<toml::Value> for TCPOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error::from("invalid type"));
        };
        Ok(Self {})
    }
}

#[derive(Debug, Default, Clone)]
pub struct QUICOptions {}

impl TryFrom<toml::Value> for QUICOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error::from("invalid type"));
        };
        Ok(Self {})
    }
}

#[derive(Debug, Default, Clone)]
pub struct UDPOptions {}

impl TryFrom<toml::Value> for UDPOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error::from("invalid type"));
        };
        Ok(Self {})
    }
}

#[derive(Debug, Default, Clone)]
pub struct IPOptions {}

impl TryFrom<toml::Value> for IPOptions {
    type Error = Error;
    fn try_from(value: toml::Value) -> Result<Self> {
        let toml::Value::Table(mut protocol) = value else {
            return Err(Error::from("invalid type"));
        };
        Ok(Self {})
    }
}

#[derive(Debug)]
pub enum Step {
    HTTP(HTTPRequest),
    HTTP1(HTTP1Request),
    HTTP2(HTTP2Request),
    HTTP3(HTTP3Request),
    TLS(TLSRequest),
    TCP(TCPRequest),
    //UDP {
    //    udp: UDPRequest,
    //},
    GraphQL(GraphQLRequest),
    //GRPC {
    //    grpc: GRPCRequest,
    //    http2: HTTP2Request,
    //},
}

impl Step {
    pub fn from_table(mut table: Table, mut defaults: &Table) -> Result<Self> {
        // Determine the step to use for the protocols specified.
        Ok(if let Some(mut http) = table.remove("http") {
            if let Some(d) = defaults.get("http") {
                merge_toml(&mut http, &d);
            }
            Step::HTTP(HTTPRequest::try_from(http)?)
        } else if let Some(mut http) = table.remove("http11") {
            if let Some(d) = defaults.get("http11") {
                merge_toml(&mut http, &d);
            }
            Step::HTTP1(HTTP1Request {
                http: HTTPRequest::try_from(http)?,
                tls: table
                    .remove("tls")
                    .map(|t| TLSOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                tcp: table
                    .remove("tcp")
                    .map(|t| TCPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                ip: table
                    .remove("ip")
                    .map(|t| IPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
            })
        } else if let Some(mut http) = table.remove("http2") {
            if let Some(d) = defaults.get("http2") {
                merge_toml(&mut http, &d);
            }
            Step::HTTP2(HTTP2Request {
                http: HTTPRequest::try_from(http)?,
                tls: table
                    .remove("tls")
                    .map(|t| TLSOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                tcp: table
                    .remove("tcp")
                    .map(|t| TCPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                ip: table
                    .remove("ip")
                    .map(|t| IPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
            })
        } else if let Some(mut http) = table.remove("http3") {
            if let Some(d) = defaults.get("http3") {
                merge_toml(&mut http, &d);
            }
            Step::HTTP3(HTTP3Request {
                http: HTTPRequest::try_from(http)?,
                tls: table
                    .remove("tls")
                    .map(|t| TLSOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                quic: table
                    .remove("quic")
                    .map(|t| QUICOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                udp: table
                    .remove("udp")
                    .map(|t| UDPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
                ip: table
                    .remove("ip")
                    .map(|t| IPOptions::try_from(t))
                    .transpose()?
                    .unwrap_or_default(),
            })
        } else if let Some(mut gql) = table.remove("graphql") {
            if let Some(d) = defaults.get("graphql") {
                merge_toml(&mut gql, &d);
            }
            Step::GraphQL(GraphQLRequest::try_from(gql)?)
        } else if let Some(mut tcp) = table.remove("tcp") {
            if let Some(d) = defaults.get("tcp") {
                merge_toml(&mut tcp, &d);
            }
            Step::TCP(TCPRequest::try_from(tcp)?)
        } else if let Some(mut tls) = table.remove("tls") {
            if let Some(d) = defaults.get("tls") {
                merge_toml(&mut tls, &d);
            }
            Step::TLS(TLSRequest::try_from(tls)?)
        } else if let Some(mut gql) = table.remove("graphql") {
            if let Some(d) = defaults.get("graphql") {
                merge_toml(&mut gql, &d);
            }
            Step::GraphQL(GraphQLRequest::try_from(gql)?)
        } else {
            return Err(Error::from("no matching protocols"));
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum PlanValue<E: StdError, T: TryFrom<String, Error = E> + Clone> {
    Literal(T),
    Dynamic {
        template: String,
        vars: Vec<(String, String)>,
    },
}

impl<E: StdError, T: TryFrom<String, Error = E> + Clone + Default> Default for PlanValue<E, T> {
    fn default() -> Self {
        PlanValue::Literal(T::default())
    }
}

impl<E: StdError, T: TryFrom<String, Error = E> + Clone> PlanValue<E, T> {
    pub fn evaluate<'a, S, O, I>(&self, state: &S) -> Result<T>
    where
        O: Into<&'a str>,
        S: State<'a, O, I>,
        I: IntoIterator<Item = O>,
    {
        match self.to_owned() {
            PlanValue::Literal(s) => Ok(s.clone()),
            Self::Dynamic { template, vars } => {
                let program = Program::compile(template).map_err(|e| Error(e.to_string()))?;
                let mut context = Context::default();
                context.add_variable(
                    "vars",
                    vars.into_iter()
                        .map(|(name, value)| (name.clone().into(), value.clone().into()))
                        .collect::<HashMap<cel_interpreter::objects::Key, cel_interpreter::Value>>(
                        ),
                );
                add_state_to_context(state, &mut context);
                match program.execute(&context) {
                    Ok(cel_interpreter::Value::String(s)) => {
                        Ok(T::try_from(s.to_string()).map_err(|e| Error(e.to_string()))?)
                    }
                    // TODO: allow arbitary bytes in all string fields instead of converting to
                    // utf8 strings?
                    Ok(cel_interpreter::Value::Bytes(b)) => Ok(T::try_from(
                        String::from_utf8(b.to_vec()).map_err(|e| Error(e.to_string()))?,
                    )
                    .map_err(|e| Error(e.to_string()))?),
                    Ok(value) => Err(Error(format!("invalid result type {:?}", value))),
                    Err(e) => Err(Error(e.to_string())),
                }
            }
        }
    }

    fn vars_from_toml(value: toml::Value) -> Result<Vec<(String, String)>> {
        if let toml::Value::Table(vars) = value {
            Ok(vars
                .into_iter()
                .map(|(name, value)| {
                    let plan_value = match value {
                        toml::Value::String(s) => s,
                        _ => return Err(Error(format!("invalid _vars.{}", name))),
                    };
                    Ok((name, plan_value))
                })
                .collect::<Result<_>>()?)
        } else {
            Err("invalid _vars".into())
        }
    }
}

impl<E: StdError, T: TryFrom<String, Error = E> + Clone> PlanValue<E, T> {
    fn try_from_value(val: toml::Value) -> Result<Option<Self>> {
        match val {
            toml::Value::String(s) => Ok(Some(Self::Literal(
                s.try_into().map_err(|e: E| Error(e.to_string()))?,
            ))),
            toml::Value::Table(mut t) => {
                if let Some(toml::Value::Boolean(unset)) = t.remove("unset") {
                    if unset {
                        return Ok(None);
                    }
                }
                if let Some(toml::Value::String(s)) = t.remove("template") {
                    Ok(Some(Self::Dynamic {
                        template: s,
                        vars: t
                            .remove("vars")
                            .map(Self::vars_from_toml)
                            .transpose()?
                            .unwrap_or_default(),
                    }))
                } else if let Some(toml::Value::String(s)) = t.remove("value") {
                    Ok(Some(Self::Literal(
                        s.try_into().map_err(|e: E| Error(e.to_string()))?,
                    )))
                } else {
                    Err(format!(
                        "value long-form must have unset, template, or value set {:?}",
                        t,
                    )
                    .into())
                }
            }
            _ => return Err("invalid value".into()),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct PlanValueTable(pub Vec<(PlanValue<Infallible, String>, PlanValue<Infallible, String>)>);

impl PlanValueTable {
    pub fn evaluate<'a, O, S, I>(&self, state: &S) -> Result<Vec<(String, String)>>
    where
        O: Into<&'a str>,
        S: State<'a, O, I>,
        I: IntoIterator<Item = O>,
    {
        self.0
            .iter()
            .map(|(key, val)| Ok((key.evaluate(state)?, val.evaluate(state)?)))
            .collect()
    }
}

impl TryFrom<toml::Value> for PlanValueTable {
    type Error = Error;
    fn try_from(val: toml::Value) -> Result<Self> {
        Ok(PlanValueTable(match val {
            // Array syntax [{ key = "foo", value = "bar" }]
            toml::Value::Array(a) => a
                .into_iter()
                .map(|val| {
                    let toml::Value::Table(mut t) = val else {
                        return Err(Error::from("invalid type"));
                    };
                    let key = t
                        .remove("key")
                        .map(PlanValue::try_from_value)
                        .transpose()?
                        .flatten()
                        .ok_or_else(|| Error::from("key is required"))?;
                    // The value can't just be missing entirely, but if the { unset = true } syntax
                    // is used we want to filter out the whole entry.
                    let value = t
                        .remove("value")
                        .ok_or_else(|| Error::from("value is required"))?;
                    let Some(value) = PlanValue::try_from_value(value)? else {
                        return Ok(None);
                    };

                    Ok(Some((key, value)))
                })
                .filter_map(Result::transpose)
                .collect::<Result<_>>()?,
            // Table syntax { foo = "bar", foobar = ["foo", "bar"] }
            toml::Value::Table(t) => t
                .into_iter()
                .filter_map(|(name, mut value)| {
                    Some(Ok((
                        match Self::leaf_to_key_value(name, &mut value) {
                            Ok(key) => key,
                            Err(e) => return Some(Err(e)),
                        },
                        match value {
                            toml::Value::Array(list) => {
                                let result: Result<Vec<_>> = list
                                    .into_iter()
                                    .map(PlanValue::try_from_value)
                                    .filter_map(Result::transpose)
                                    .collect();
                                match result {
                                    Ok(list) => list,
                                    Err(e) => return Some(Err(e)),
                                }
                            }
                            value => vec![match PlanValue::try_from_value(value) {
                                Ok(Some(pv)) => pv,
                                // If there's no value then it was omitted with { unset = true },
                                // so filter the whole entry out.
                                Ok(None) => return None,
                                Err(e) => return Some(Err(e)),
                            }],
                        },
                    )))
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                // Flat map the array from [(key, [foo, bar])] to [(key, foo), (key, bar)]
                .flat_map(|(name, values)| repeat(name).zip(values.into_iter()))
                .collect(),
            _ => return Err(Error::from("invalid map")),
        }))
    }
}

impl PlanValueTable {
    fn leaf_to_key_value(
        key: String,
        value: &mut toml::Value,
    ) -> Result<PlanValue<Infallible, String>> {
        match value {
            // Strings or array values mean the key is not templated.
            toml::Value::String(_) | toml::Value::Array(_) => Ok(PlanValue::Literal(key)),
            // If the value is a table, check for the appropriate option to decide if the key is
            // templated.
            toml::Value::Table(t) => match t.remove("key_is_template") {
                Some(toml::Value::Boolean(b)) if b => Ok(PlanValue::Dynamic {
                    template: key,
                    vars: t
                        .get("vars")
                        .map(toml::Value::to_owned)
                        .map(PlanValue::<Infallible, String>::vars_from_toml)
                        .transpose()?
                        .unwrap_or_default(),
                }),
                Some(toml::Value::Boolean(_)) | None => Ok(PlanValue::Literal(key)),
                _ => return Err(Error(format!("{}.key_is_template invalid", key))),
            },
            _ => return Err(Error(format!("{} has invalid type", key))),
        }
    }
}

fn add_state_to_context<'a, S, O, I>(state: &S, ctx: &mut cel_interpreter::Context)
where
    O: Into<&'a str>,
    S: State<'a, O, I>,
    I: IntoIterator<Item = O>,
{
    for name in state.iter() {
        let name = name.into();
        let output = state.get(name).unwrap();
        ctx.add_variable(name, output);
    }
}
