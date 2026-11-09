//! EWS replies for the parser tests (#369), shaped after what Exchange 2016
//! and 2019 send: an `s:` envelope with a `ServerVersionInfo` header, `m:`
//! and `t:` declared on the response element, every message carrying its
//! `ResponseClass` and `ResponseCode`. Variants spell the same replies with
//! other prefixes and default namespaces, and the broken ones are what a
//! reverse proxy, a cut connection or a failing server sends instead.

/// The SOAP 1.1 envelope Exchange wraps every reply in, around `body`.
/// `m:` and `t:` are declared on the envelope, so fragments can use them.
pub fn envelope(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"><s:Header><h:ServerVersionInfo MajorVersion="15" MinorVersion="2" MajorBuildNumber="1118" MinorBuildNumber="7" Version="V2017_07_11" xmlns:h="http://schemas.microsoft.com/exchange/services/2006/types"/></s:Header><s:Body>{body}</s:Body></s:Envelope>"#
    )
}

/// FindFolder, as Exchange 2019 writes it.
pub const FIND_FOLDER_SOAP_PREFIX: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header>
    <h:ServerVersionInfo MajorVersion="15" MinorVersion="2" MajorBuildNumber="1118" MinorBuildNumber="7" Version="V2017_07_11" xmlns:h="http://schemas.microsoft.com/exchange/services/2006/types" xmlns="http://schemas.microsoft.com/exchange/services/2006/types" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/>
  </s:Header>
  <s:Body xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
    <m:FindFolderResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:FindFolderResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:RootFolder IndexedPagingOffset="2" TotalItemsInView="2" IncludesLastItemInRange="true">
            <t:Folders>
              <t:CalendarFolder>
                <t:FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ==" ChangeKey="AgAAABYAAAD/"/>
                <t:DisplayName>Calendar</t:DisplayName>
                <t:FolderClass>IPF.Appointment</t:FolderClass>
              </t:CalendarFolder>
              <t:CalendarFolder>
                <t:FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAg==" ChangeKey="AgAAABYAAAEA"/>
                <t:DisplayName>Sales &amp; Marketing</t:DisplayName>
                <t:FolderClass>IPF.Appointment</t:FolderClass>
              </t:CalendarFolder>
            </t:Folders>
          </m:RootFolder>
        </m:FindFolderResponseMessage>
      </m:ResponseMessages>
    </m:FindFolderResponse>
  </s:Body>
</s:Envelope>"##;

/// [`FIND_FOLDER_SOAP_PREFIX`] with no prefix at all: every level declares
/// its namespace as the default one.
pub const FIND_FOLDER_DEFAULT_NAMESPACES: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<Envelope xmlns="http://schemas.xmlsoap.org/soap/envelope/">
  <Body>
    <FindFolderResponse xmlns="http://schemas.microsoft.com/exchange/services/2006/messages">
      <ResponseMessages>
        <FindFolderResponseMessage ResponseClass="Success">
          <ResponseCode>NoError</ResponseCode>
          <RootFolder TotalItemsInView="2" IncludesLastItemInRange="true">
            <Folders xmlns="http://schemas.microsoft.com/exchange/services/2006/types">
              <CalendarFolder>
                <FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ==" ChangeKey="AgAAABYAAAD/"/>
                <DisplayName>Calendar</DisplayName>
              </CalendarFolder>
              <CalendarFolder>
                <FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAg==" ChangeKey="AgAAABYAAAEA"/>
                <DisplayName><![CDATA[Sales & Marketing]]></DisplayName>
              </CalendarFolder>
            </Folders>
          </RootFolder>
        </FindFolderResponseMessage>
      </ResponseMessages>
    </FindFolderResponse>
  </Body>
</Envelope>"##;

/// [`FIND_FOLDER_SOAP_PREFIX`] with prefixes no Exchange uses, declared
/// once on the envelope.
pub const FIND_FOLDER_ODD_PREFIXES: &str = r##"<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:msg="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:typ="http://schemas.microsoft.com/exchange/services/2006/types"><soapenv:Body><msg:FindFolderResponse><msg:ResponseMessages><msg:FindFolderResponseMessage ResponseClass="Success"><msg:ResponseCode>NoError</msg:ResponseCode><msg:RootFolder TotalItemsInView="2" IncludesLastItemInRange="true"><typ:Folders><typ:CalendarFolder><typ:FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ==" ChangeKey="AgAAABYAAAD/"/><typ:DisplayName>Calendar</typ:DisplayName></typ:CalendarFolder><typ:CalendarFolder><typ:FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAg==" ChangeKey="AgAAABYAAAEA"/><typ:DisplayName>Sales &#38; Marketing</typ:DisplayName></typ:CalendarFolder></typ:Folders></msg:RootFolder></msg:FindFolderResponseMessage></msg:ResponseMessages></msg:FindFolderResponse></soapenv:Body></soapenv:Envelope>"##;

/// A CalendarView FindItem as Exchange 2016 writes it: a single event, an
/// all-day absence and an occurrence of a recurring series, with entities
/// and a character reference in the text.
pub const FIND_ITEM_CALENDAR_VIEW: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Header>
    <h:ServerVersionInfo MajorVersion="15" MinorVersion="1" MajorBuildNumber="2507" MinorBuildNumber="6" Version="V2017_07_11" xmlns:h="http://schemas.microsoft.com/exchange/services/2006/types" xmlns="http://schemas.microsoft.com/exchange/services/2006/types" xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"/>
  </s:Header>
  <s:Body xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
    <m:FindItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:FindItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:RootFolder TotalItemsInView="3" IncludesLastItemInRange="true">
            <t:Items>
              <t:CalendarItem>
                <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAA=" ChangeKey="DwAAABYAAAD/"/>
                <t:Subject>Q3 review: R&amp;D &lt;budget&gt; &#8211; draft</t:Subject>
                <t:Start>2026-05-06T09:00:00Z</t:Start>
                <t:End>2026-05-06T09:30:00Z</t:End>
                <t:IsAllDayEvent>false</t:IsAllDayEvent>
                <t:LegacyFreeBusyStatus>Busy</t:LegacyFreeBusyStatus>
                <t:Location>Room "Turing" &amp; Teams</t:Location>
                <t:IsCancelled>false</t:IsCancelled>
                <t:CalendarItemType>Single</t:CalendarItemType>
                <t:UID>040000008200E00074C5B7101A82E00800000000A1B2C3D4E1DC0100000000000000001000000011</t:UID>
              </t:CalendarItem>
              <t:CalendarItem>
                <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAB=" ChangeKey="DwAAABYAAAEA"/>
                <t:Subject>Out of office</t:Subject>
                <t:Start>2026-05-08T00:00:00Z</t:Start>
                <t:End>2026-05-09T00:00:00Z</t:End>
                <t:IsAllDayEvent>true</t:IsAllDayEvent>
                <t:LegacyFreeBusyStatus>OOF</t:LegacyFreeBusyStatus>
                <t:IsCancelled>false</t:IsCancelled>
                <t:CalendarItemType>Single</t:CalendarItemType>
                <t:UID>uid-2@corp.example</t:UID>
              </t:CalendarItem>
              <t:CalendarItem>
                <t:ItemId Id="AAMkADUwNjA3YTQ5AFEAAAAAAD+/b8xAAC=" ChangeKey="DwAAABYAAAEB"/>
                <t:Subject>Daily standup</t:Subject>
                <t:Start>2026-05-07T07:45:00Z</t:Start>
                <t:End>2026-05-07T08:00:00Z</t:End>
                <t:IsAllDayEvent>false</t:IsAllDayEvent>
                <t:LegacyFreeBusyStatus>Tentative</t:LegacyFreeBusyStatus>
                <t:IsCancelled>true</t:IsCancelled>
                <t:CalendarItemType>Occurrence</t:CalendarItemType>
                <t:UID>series-master@corp.example</t:UID>
              </t:CalendarItem>
            </t:Items>
          </m:RootFolder>
        </m:FindItemResponseMessage>
      </m:ResponseMessages>
    </m:FindItemResponse>
  </s:Body>
</s:Envelope>"##;

/// [`FIND_ITEM_CALENDAR_VIEW`] cut off mid-item, as a dropped connection
/// leaves it. Read as complete, the events after the cut would look gone.
pub const TRUNCATED_FIND_ITEM: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:FindItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:FindItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:RootFolder TotalItemsInView="3" IncludesLastItemInRange="true">
            <t:Items>
              <t:CalendarItem>
                <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAA=" ChangeKey="DwAAABYAAAD/"/>
                <t:Subject>Q3 rev"##;

/// What a reverse proxy in front of Exchange answers with a 200 when its
/// session expired: a sign-in page, not a SOAP reply.
pub const HTML_SIGN_IN_PAGE: &str = r##"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>Sign in</title></head>
<body><form method="post" action="/owa/auth.owa"><input name="username"><br>
<p>Fault? ResponseCode NoError Envelope</p></form></body>
</html>"##;

/// A FindItem whose event text quotes SOAP markup, escaped and in CDATA:
/// none of it is a fault.
pub const FIND_ITEM_FAULT_LOOKALIKE_TEXT: &str = r##"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><m:FindItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"><m:ResponseMessages><m:FindItemResponseMessage ResponseClass="Success"><m:ResponseCode>NoError</m:ResponseCode><m:RootFolder TotalItemsInView="1" IncludesLastItemInRange="true"><t:Items><t:CalendarItem><t:ItemId Id="AAA=" ChangeKey="CK"/><t:Subject>&lt;s:Fault&gt;&lt;faultstring&gt;postmortem&lt;/faultstring&gt;&lt;/s:Fault&gt;</t:Subject><t:Location><![CDATA[<m:ResponseCode>ErrorAccessDenied</m:ResponseCode>]]></t:Location><t:Start>2026-05-06T09:00:00Z</t:Start><t:End>2026-05-06T10:00:00Z</t:End></t:CalendarItem></t:Items></m:RootFolder></m:FindItemResponseMessage></m:ResponseMessages></m:FindItemResponse></s:Body></s:Envelope>"##;

/// The fault Exchange answers when the service account lacks the
/// `ApplicationImpersonation` role (#138). Exchange 2016 sends it with a
/// 500, some front ends with a 200: both are faults.
pub const IMPERSONATE_USER_DENIED: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <s:Fault>
      <faultcode xmlns:a="http://schemas.microsoft.com/exchange/services/2006/types">a:ErrorImpersonateUserDenied</faultcode>
      <faultstring xml:lang="en-US">The account does not have permission to impersonate the requested user.</faultstring>
      <detail>
        <e:ResponseCode xmlns:e="http://schemas.microsoft.com/exchange/services/2006/errors">ErrorImpersonateUserDenied</e:ResponseCode>
        <e:Message xmlns:e="http://schemas.microsoft.com/exchange/services/2006/errors">The account does not have permission to impersonate the requested user.</e:Message>
      </detail>
    </s:Fault>
  </s:Body>
</s:Envelope>"##;

/// The fault for an impersonated address with no mailbox, from a French
/// Exchange: the reason is localised, the code is not.
pub const NON_EXISTENT_MAILBOX_FR: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <s:Fault>
      <faultcode xmlns:a="http://schemas.microsoft.com/exchange/services/2006/types">a:ErrorNonExistentMailbox</faultcode>
      <faultstring xml:lang="fr-FR">Aucune boîte aux lettres n&apos;est associée à l&apos;adresse SMTP.</faultstring>
      <detail>
        <e:ResponseCode xmlns:e="http://schemas.microsoft.com/exchange/services/2006/errors">ErrorNonExistentMailbox</e:ResponseCode>
        <e:Message xmlns:e="http://schemas.microsoft.com/exchange/services/2006/errors">Aucune boîte aux lettres n&apos;est associée à l&apos;adresse SMTP.</e:Message>
        <t:MessageXml xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
          <t:Value Name="SmtpAddress">nobody@corp.example</t:Value>
        </t:MessageXml>
      </detail>
    </s:Fault>
  </s:Body>
</s:Envelope>"##;

/// A SOAP 1.2 fault, which a front end speaking 1.2 may return.
pub const SOAP12_FAULT: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope">
  <env:Body>
    <env:Fault>
      <env:Code><env:Value>env:Sender</env:Value></env:Code>
      <env:Reason><env:Text xml:lang="en-US">The request failed schema validation.</env:Text></env:Reason>
      <env:Detail>
        <e:ResponseCode xmlns:e="http://schemas.microsoft.com/exchange/services/2006/errors">ErrorSchemaValidation</e:ResponseCode>
      </env:Detail>
    </env:Fault>
  </env:Body>
</env:Envelope>"##;

/// A per-message error in a 200, the way Exchange refuses a folder the
/// account cannot read.
pub const ACCESS_DENIED_MESSAGE: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:FindItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:FindItemResponseMessage ResponseClass="Error">
          <m:MessageText>Access is denied. Check credentials and try again.</m:MessageText>
          <m:ResponseCode>ErrorAccessDenied</m:ResponseCode>
          <m:DescriptiveLinkKey>0</m:DescriptiveLinkKey>
        </m:FindItemResponseMessage>
      </m:ResponseMessages>
    </m:FindItemResponse>
  </s:Body>
</s:Envelope>"##;

/// The base64 MIME of [`GET_ITEM_MIME`]: a message carrying a weekly
/// VEVENT.
pub const GET_ITEM_MIME_B64: &str = "UmVjZWl2ZWQ6IGZyb20gbWFpbC5jb3JwLmV4YW1wbGUNCkNvbnRlbnQtVHlwZTogdGV4dC9jYWxlbmRhcjsgY2hhcnNldD0idXRmLTgiOyBtZXRob2Q9UkVRVUVTVA0KU3ViamVjdDogQm9hcmQgcmV2aWV3DQpNSU1FLVZlcnNpb246IDEuMA0KDQpCRUdJTjpWQ0FMRU5EQVINCk1FVEhPRDpSRVFVRVNUDQpQUk9ESUQ6TWljcm9zb2Z0IEV4Y2hhbmdlIFNlcnZlciAyMDEwDQpWRVJTSU9OOjIuMA0KQkVHSU46VkVWRU5UDQpVSUQ6MDQwMDAwMDA4MjAwRTAwMDc0QzVCNzEwMUE4MkUwMDgwMDAwMDAwMDEwQjRENkMyRDRFMURDMDEwMDAwMDAwMDAwMDAwMDAwMTAwMDAwMDANClNVTU1BUlk7TEFOR1VBR0U9ZW4tVVM6Qm9hcmQgcmV2aWV3ICYgYnVkZ2V0DQpEVFNUQVJUOjIwMjYwNTEyVDA5MDAwMFoNCkRURU5EOjIwMjYwNTEyVDEwMDAwMFoNClJSVUxFOkZSRVE9V0VFS0xZO0NPVU5UPTQNCkVORDpWRVZFTlQNCkVORDpWQ0FMRU5EQVINCg==";

/// GetItem with MIME content for two items, the second a meeting request.
pub const GET_ITEM_MIME: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:CalendarItem>
              <t:MimeContent CharacterSet="UTF-8">UmVjZWl2ZWQ6IGZyb20gbWFpbC5jb3JwLmV4YW1wbGUNCkNvbnRlbnQtVHlwZTogdGV4dC9jYWxlbmRhcjsgY2hhcnNldD0idXRmLTgiOyBtZXRob2Q9UkVRVUVTVA0KU3ViamVjdDogQm9hcmQgcmV2aWV3DQpNSU1FLVZlcnNpb246IDEuMA0KDQpCRUdJTjpWQ0FMRU5EQVINCk1FVEhPRDpSRVFVRVNUDQpQUk9ESUQ6TWljcm9zb2Z0IEV4Y2hhbmdlIFNlcnZlciAyMDEwDQpWRVJTSU9OOjIuMA0KQkVHSU46VkVWRU5UDQpVSUQ6MDQwMDAwMDA4MjAwRTAwMDc0QzVCNzEwMUE4MkUwMDgwMDAwMDAwMDEwQjRENkMyRDRFMURDMDEwMDAwMDAwMDAwMDAwMDAwMTAwMDAwMDANClNVTU1BUlk7TEFOR1VBR0U9ZW4tVVM6Qm9hcmQgcmV2aWV3ICYgYnVkZ2V0DQpEVFNUQVJUOjIwMjYwNTEyVDA5MDAwMFoNCkRURU5EOjIwMjYwNTEyVDEwMDAwMFoNClJSVUxFOkZSRVE9V0VFS0xZO0NPVU5UPTQNCkVORDpWRVZFTlQNCkVORDpWQ0FMRU5EQVINCg==</t:MimeContent>
              <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAA=" ChangeKey="DwAAABYAAAD/"/>
              <t:UID>040000008200E00074C5B7101A82E0080000000010B4D6C2D4E1DC01000000000000000010000000</t:UID>
            </t:CalendarItem>
          </m:Items>
        </m:GetItemResponseMessage>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:MeetingRequest>
              <t:MimeContent CharacterSet="UTF-8">
UmVjZWl2ZWQ6IGZyb20gbWFpbC5jb3JwLmV4YW1wbGUNCkNvbnRlbnQtVHlwZTogdGV4dC9jYWxlbmRhcjsgY2hhcnNldD0idXRmLTgiOyBtZXRob2Q9UkVRVUVTVA0KU3ViamVjdDogQm9hcmQgcmV2aWV3DQpNSU1FLVZlcnNpb246IDEuMA0KDQpCRUdJTjpWQ0FMRU5EQVINCk1FVEhPRDpSRVFVRVNUDQpQUk9ESUQ6TWljcm9zb2Z0IEV4Y2hhbmdlIFNlcnZlciAyMDEwDQpWRVJTSU9OOjIuMA0KQkVHSU46VkVWRU5UDQpVSUQ6MDQwMDAwMDA4MjAwRTAwMDc0QzVCNzEwMUE4MkUwMDgwMDAwMDAwMDEwQjRENkMyRDRFMURDMDEwMDAwMDAwMDAwMDAwMDAwMTAwMDAwMDANClNVTU1BUlk7TEFOR1VBR0U9ZW4tVVM6Qm9hcmQgcmV2aWV3ICYgYnVkZ2V0DQpEVFNUQVJUOjIwMjYwNTEyVDA5MDAwMFoNCkRURU5EOjIwMjYwNTEyVDEwMDAwMFoNClJSVUxFOkZSRVE9V0VFS0xZO0NPVU5UPTQNCkVORDpWRVZFTlQNCkVORDpWQ0FMRU5EQVINCg==
              </t:MimeContent>
              <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xAAB=" ChangeKey="CwAAABYAAAEA"/>
            </t:MeetingRequest>
          </m:Items>
        </m:GetItemResponseMessage>
      </m:ResponseMessages>
    </m:GetItemResponse>
  </s:Body>
</s:Envelope>"##;

/// A GetItem batch whose second item is gone.
pub const GET_ITEM_SECOND_MESSAGE_FAILS: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:GetItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items><t:CalendarItem><t:ItemId Id="AAA=" ChangeKey="CK"/></t:CalendarItem></m:Items>
        </m:GetItemResponseMessage>
        <m:GetItemResponseMessage ResponseClass="Error">
          <m:MessageText>The specified object was not found in the store., The process failed to get the correct properties.</m:MessageText>
          <m:ResponseCode>ErrorItemNotFound</m:ResponseCode>
          <m:DescriptiveLinkKey>0</m:DescriptiveLinkKey>
          <m:Items/>
        </m:GetItemResponseMessage>
      </m:ResponseMessages>
    </m:GetItemResponse>
  </s:Body>
</s:Envelope>"##;

/// CreateItem of a calendar item.
pub const CREATE_ITEM: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:CreateItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:CreateItemResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Items>
            <t:CalendarItem>
              <t:ItemId Id="AAMkADUwNjA3YTQ5AEYAAAAAAD+/b8xNEW=" ChangeKey="DwAAABYAAAEC"/>
            </t:CalendarItem>
          </m:Items>
        </m:CreateItemResponseMessage>
      </m:ResponseMessages>
    </m:CreateItemResponse>
  </s:Body>
</s:Envelope>"##;

/// GetFolder of the well-known calendar folder.
pub const GET_FOLDER_CALENDAR: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:GetFolderResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:GetFolderResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:Folders>
            <t:CalendarFolder>
              <t:FolderId Id="AAMkADUwNjA3YTQ5LTEzZDktNGQ2Yy1iZDk3LTJiNzRkZGFiMmJmZgAuAAAAAAA+/b8xAQ==" ChangeKey="AgAAABYAAAD/"/>
            </t:CalendarFolder>
          </m:Folders>
        </m:GetFolderResponseMessage>
      </m:ResponseMessages>
    </m:GetFolderResponse>
  </s:Body>
</s:Envelope>"##;

/// DeleteItem that succeeded.
pub const DELETE_ITEM: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body><m:DeleteItemResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types"><m:ResponseMessages><m:DeleteItemResponseMessage ResponseClass="Success"><m:ResponseCode>NoError</m:ResponseCode></m:DeleteItemResponseMessage></m:ResponseMessages></m:DeleteItemResponse></s:Body></s:Envelope>"##;

/// SyncFolderItems with one change of each kind calrs reads, plus a read
/// flag change it ignores.
pub const SYNC_FOLDER_ITEMS: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/">
  <s:Body>
    <m:SyncFolderItemsResponse xmlns:m="http://schemas.microsoft.com/exchange/services/2006/messages" xmlns:t="http://schemas.microsoft.com/exchange/services/2006/types">
      <m:ResponseMessages>
        <m:SyncFolderItemsResponseMessage ResponseClass="Success">
          <m:ResponseCode>NoError</m:ResponseCode>
          <m:SyncState>H4sIAAAAAAAEAO29B2AcSZYlJi9tynt/SvVK1+B0oQiAYBMk2JBAEOzBiM3mkuwdaUcjKasqgcplVmVdZhZAzO2dvPfee++999577733/vMzx+</m:SyncState>
          <m:IncludesLastItemInRange>true</m:IncludesLastItemInRange>
          <m:Changes>
            <t:Create>
              <t:CalendarItem>
                <t:ItemId Id="N1=" ChangeKey="CK1"/>
                <t:UID>uid-new@corp.example</t:UID>
              </t:CalendarItem>
            </t:Create>
            <t:Update>
              <t:CalendarItem>
                <t:ItemId Id="N2=" ChangeKey="CK2"/>
                <t:UID>uid-upd&amp;co@corp.example</t:UID>
              </t:CalendarItem>
            </t:Update>
            <t:ReadFlagChange>
              <t:ItemId Id="N3=" ChangeKey="CK3"/>
              <t:IsRead>true</t:IsRead>
            </t:ReadFlagChange>
            <t:Delete>
              <t:ItemId Id="GONE=" ChangeKey="CK4"/>
            </t:Delete>
          </m:Changes>
        </m:SyncFolderItemsResponseMessage>
      </m:ResponseMessages>
    </m:SyncFolderItemsResponse>
  </s:Body>
</s:Envelope>"##;
